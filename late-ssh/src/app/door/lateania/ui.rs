// Rendering for Lateania. Reads the cached per-session snapshot and paints a
// two-column view: the scrolling adventure log on the left, a context side panel
// on the right (room / character / abilities / inventory / shop). Before a class
// is chosen it shows the class-selection screen. Lock-free; never awaits.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
use uuid::Uuid;

use crate::app::common::theme;
use crate::usernames::UsernameLookup;

use super::{
    appearance,
    classes::Class,
    pets::{MEALS_PER_DAY, PET_MAX_LEVEL},
    state::{ClickAction, Heading, InvAction, MapMode, Panel, State, inv_action},
    stats::{POINT_EVERY_LEVELS, SCORE_CAP, Score},
    svc::{
        CraftView, InvView, LeaderboardEntry, LogKind, MobView, PetView, PlayerView, QuestKind,
        QuestView, SectionRow, ShopView,
    },
    world::{Dir, MapCell, MiniMap, RoomId},
};

const SIDE_WIDE: u16 = 34;
const SIDE_NARROW: u16 = 28;
/// The widest the side rail grows on a big terminal. Past this the room text
/// stops reading as a column and the main view starts paying for it.
const SIDE_MAX: u16 = 60;
/// Columns the standing-key block is indented by. Anything sized against the
/// rail has to subtract it, or the line it builds is a couple of columns wider
/// than the space it is painted into and gets chopped at the edge.
const RAIL_INDENT: usize = 2;

/// How many rows the bottom log strip gets on a terminal this tall.
///
/// It used to be `height / 4` capped at 7, so a tall terminal showed the same
/// four-or-so events a short one did: room description, one look, and the text
/// had already scrolled away. A third of the height, capped at 12, keeps a
/// readable run of events on a big screen while still leaving the field the
/// larger share, and short terminals are unchanged in practice.
fn log_strip_height(total_height: u16) -> u16 {
    (total_height / 3).clamp(4, 12)
}

/// How wide the side rail gets for a terminal this wide.
///
/// It used to be 34 columns flat above a single threshold, whatever the screen:
/// on a 190-column terminal that spent a third of the panel's *height* on wrap
/// damage (one exits line wrapping to three, a wildlife entry to three) while
/// the main column sat half empty. A quarter of the width, bounded, keeps the
/// rail a column on a laptop and lets it breathe on a wide display.
fn side_width(total_width: u16) -> u16 {
    if total_width < 84 {
        return SIDE_NARROW;
    }
    (total_width / 4).clamp(SIDE_WIDE, SIDE_MAX)
}

/// Whether the field layout's message feed rides in the side rail instead of
/// the full-width strip under the field.
///
/// The strip is the better shape when there are rows to spend on it: log lines
/// are sentences, and sentences want width. On a short terminal - a phone held
/// sideways is twenty rows if you are lucky - it is the wrong trade, because
/// the strip's `height / 3` is taken off the one thing that cannot be scrolled
/// back to: the live field. Below this the events move into the rail, which is
/// already a column of wrapped text, and the field keeps the full height.
fn events_in_rail(total_height: u16) -> bool {
    total_height < 24
}

/// Rows the rail's events block gets: its bottom third, so the room summary
/// above it keeps the larger share of a rail that is already narrow.
fn rail_log_height(rail_height: u16) -> u16 {
    (rail_height / 3).clamp(3, 10)
}

// ---- Screen entry: which layout this terminal gets -----------------------

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, usernames: &UsernameLookup<'_>) {
    let view = state.view();

    if state.reset_elsewhere() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "Lateania character reset from another session.",
                    Style::default().fg(theme::AMBER_GLOW()),
                )),
                Line::from(Span::styled(
                    "Press Esc to return to Games, then enter again to start over.",
                    Style::default().fg(theme::TEXT_DIM()),
                )),
            ])
            .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }

    if !view.joined {
        frame.render_widget(
            Paragraph::new(vec![Line::from(Span::styled(
                "Entering Lateania...",
                Style::default().fg(theme::AMBER_GLOW()),
            ))]),
            area,
        );
        return;
    }

    if !view.classed {
        draw_class_select(frame, area, &view, state.class_cursor());
        return;
    }

    if !view.archetype_choices.is_empty() {
        draw_archetype_select(frame, area, &view);
        return;
    }

    if !view.score_offer.is_empty() {
        frame.render_widget(
            Paragraph::new(score_point_lines(&view, area.height)).wrap(Wrap { trim: false }),
            area,
        );
        return;
    }

    if area.width < 50 || area.height < 9 {
        draw_compact(frame, area, &view);
        return;
    }

    // The character sheet expands to the full view when there is room, for a
    // dense dashboard (portrait, dot-rated scores, vitals bars). It falls back
    // to the narrow side panel on cramped terminals.
    if state.panel() == Panel::Character && area.width >= 72 && area.height >= 18 {
        let off = draw_character_sheet(frame, area, &view, state.list_scroll());
        state.set_list_scroll(off);
        return;
    }

    // The journal and the board are text-heavy; given a wide terminal they
    // expand to full-screen column layouts the same way, and fall back to
    // the side panel when cramped. Cursor, keys, and tracking are identical
    // in both renderings.
    if state.panel() == Panel::Quests && area.width >= 100 && area.height >= 20 {
        draw_journal_screen(frame, area, state, &view);
        return;
    }
    if state.panel() == Panel::Board && area.width >= 100 && area.height >= 20 {
        draw_board_screen(frame, area, state, &view);
        return;
    }
    // The shop is the densest list in the game (stock, price, the stat line, the
    // comparison against what's worn, the description) and the side rail wraps
    // every one of those into a scroll. Given room it takes the screen.
    if state.panel() == Panel::Shop && area.width >= 100 && area.height >= 20 {
        match &view.shop {
            Some(shop) => draw_shop_screen(
                frame,
                area,
                &state.shop_rows(),
                shop,
                state.cursor(),
                view.gold,
                view.banked_gold,
            ),
            None => frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "No shop here.",
                    Style::default().fg(theme::TEXT_DIM()),
                ))),
                area,
            ),
        }
        return;
    }
    // The pack is the shop's twin: the same dense list, and the same need to
    // stand a piece against what it would replace.
    if state.panel() == Panel::Inventory && area.width >= 100 && area.height >= 20 {
        draw_inventory_screen(frame, area, &state.inv_rows(), &view, state.cursor());
        return;
    }
    // The forge is the third of them: a craft is a gear decision like a
    // purchase is, and it has an ingredient list on top of everything a
    // listing carries.
    if state.panel() == Panel::Crafting && area.width >= 100 && area.height >= 20 {
        match &view.crafting {
            Some(craft) => {
                draw_craft_screen(frame, area, &state.craft_rows(), craft, state.cursor())
            }
            None => frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "No crafting station here.",
                    Style::default().fg(theme::TEXT_DIM()),
                ))),
                area,
            ),
        }
        return;
    }

    let side_w = side_width(area.width);
    // Wide terminals get the live field with the message log as a full-width
    // strip along the bottom, the way terminal roguelikes have always laid it:
    // log lines are sentences, and sentences want width, not a narrow rail.
    // Below this width the field folds away and the classic log + side view
    // stands in (the minimap still rides in the side panel there).
    if state.panel() == Panel::Room && view.rpg_mode && area.width >= 96 {
        // A tall terminal spends rows on the full-width strip; a short one
        // gives them back to the field and pushes the events into the rail.
        let strip = match events_in_rail(area.height) {
            true => None,
            false => {
                let log_h = log_strip_height(area.height);
                let rows =
                    Layout::vertical([Constraint::Min(3), Constraint::Length(log_h)]).split(area);
                Some((rows[0], rows[1]))
            }
        };
        let field_area = strip.map_or(area, |(top, _)| top);
        let cols = Layout::horizontal([
            Constraint::Min(24),        // live field (fills the middle)
            Constraint::Length(side_w), // room summary + foes
        ])
        .split(field_area);
        draw_field(frame, cols[0], &view);
        match strip {
            Some((_, log_area)) => {
                draw_room_side(frame, cols[1], state, &view, usernames, false);
                draw_log_strip(frame, log_area, &view);
            }
            None => {
                let log_h = rail_log_height(cols[1].height);
                let rail = Layout::vertical([Constraint::Min(4), Constraint::Length(log_h)])
                    .split(cols[1]);
                draw_room_side(frame, rail[0], state, &view, usernames, false);
                draw_log_strip(frame, rail[1], &view);
            }
        }
        return;
    }

    let cols = Layout::horizontal([Constraint::Min(26), Constraint::Length(side_w)]).split(area);
    draw_log(frame, cols[0], &view);
    draw_side(frame, cols[1], state, &view, usernames);
}

// ---- The clickable action bar along the bottom row -----------------------

/// One chip on the combat action bar: its label, the action a click triggers,
/// and whether it is ready (dim when a spell can't be paid for right now).
struct Chip {
    label: String,
    action: ClickAction,
    ready: bool,
}

/// Build the action-bar chips left to right within `max_width`: Attack first,
/// then as many ability slots as fit, always keeping room for Coat, Quaff and
/// Flee on the end (the three that shouldn't cost a panel to reach). Kept pure
/// so the layout is unit-testable.
fn combat_chips(view: &PlayerView, max_width: u16) -> Vec<Chip> {
    let width_of = |s: &str| UnicodeWidthStr::width(s) as u16;
    let attack = Chip {
        label: "\u{2694} Atk".to_string(), // ⚔
        action: ClickAction::Attack,
        ready: true,
    };
    let quaff = Chip {
        label: "\u{2665} Quaff".to_string(), // ♥
        action: ClickAction::Quaff,
        ready: true,
    };
    // Shows the strikes left rather than the school: the school is already on
    // the effects line directly above, and the number is the part that decides
    // whether you press it.
    let coat = Chip {
        label: match &view.coat {
            Some(c) => format!("\u{2697} x{}", c.charges), // ⚗
            None => "\u{2697} Coat".to_string(),
        },
        action: ClickAction::Coat,
        ready: view.coat.is_none(),
    };
    let flee = Chip {
        label: "\u{2691} Flee".to_string(), // ⚑
        action: ClickAction::Flee,
        ready: true,
    };
    // Reserve the trailing Coat/Quaff/Flee (plus a space before each) so
    // abilities in the middle never crowd them off the row.
    let reserved =
        width_of(&coat.label) + 1 + width_of(&quaff.label) + 1 + width_of(&flee.label) + 1;
    let mut chips = vec![attack];
    let mut used = width_of(&chips[0].label);
    for a in &view.abilities {
        // Slot 10 is cast with `0`, matching the keybind; show that digit.
        let key = if a.slot == 10 { 0 } else { a.slot };
        let label = format!("{key} {}", truncate_chars(&a.name, 7));
        let w = width_of(&label) + 1; // leading space between chips
        if used + w + reserved > max_width {
            break;
        }
        used += w;
        chips.push(Chip {
            label,
            action: ClickAction::Ability(a.slot),
            ready: a.ready,
        });
    }
    chips.push(coat);
    chips.push(quaff);
    chips.push(flee);
    chips
}

/// The clickable combat action bar: a single row of chips whose absolute rects
/// are recorded so a click resolves to the same action as its key.
fn draw_action_bar(frame: &mut Frame, area: Rect, state: &State, view: &PlayerView) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let chips = combat_chips(view, area.width);
    let mut spans: Vec<Span> = Vec::new();
    let mut col = area.x;
    for (i, chip) in chips.iter().enumerate() {
        if i > 0 && col < area.x.saturating_add(area.width) {
            spans.push(Span::raw(" "));
            col += 1;
        }
        let w = UnicodeWidthStr::width(chip.label.as_str()) as u16;
        state.record_combat_hit(
            Rect {
                x: col,
                y: area.y,
                width: w,
                height: 1,
            },
            chip.action,
        );
        let style = match chip.action {
            ClickAction::Attack => Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
            ClickAction::Quaff => Style::default().fg(theme::SUCCESS()),
            // Dim once a coat is live: the chip is then mostly a readout of
            // what's left. It stays clickable, because a nearly spent coat
            // still tops up (`coat_best`).
            ClickAction::Coat if chip.ready => Style::default().fg(theme::AMBER()),
            ClickAction::Coat => Style::default().fg(theme::TEXT_DIM()),
            ClickAction::Flee => Style::default().fg(theme::TEXT_DIM()),
            ClickAction::Ability(_) if chip.ready => Style::default().fg(theme::AMBER()),
            ClickAction::Ability(_) => Style::default().fg(theme::TEXT_FAINT()),
            // Foe/adventurer rows carry these, never the action bar; kept for
            // exhaustiveness.
            ClickAction::AttackMob(_) | ClickAction::AttackPlayer(_) => {
                Style::default().fg(theme::TEXT_DIM())
            }
        };
        spans.push(Span::styled(chip.label.clone(), style));
        col += w;
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Truncate to `max` display columns, adding a trailing dot when clipped.
fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('.');
    out
}

pub fn draw_page(frame: &mut Frame, area: Rect, state: &State, usernames: &UsernameLookup<'_>) {
    if area.height < 4 {
        draw_game(frame, area, state, usernames);
        return;
    }

    let rows = Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).split(area);
    let view = state.view();
    let title = if view.classed {
        format!(
            "LATEANIA BBS DOOR  |  {} lvl {}  |  {} adventurers online",
            view.class_name,
            view.level,
            state.player_count()
        )
    } else {
        format!(
            "LATEANIA BBS DOOR  |  persistent server world  |  {} online",
            state.player_count()
        )
    };
    let mut title_lines = vec![Line::from(vec![Span::styled(
        title,
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD),
    )])];
    if state.leave_confirm_pending() {
        title_lines.push(Line::from(Span::styled(
            "Press Esc again to leave Lateania - any other key stays.",
            Style::default()
                .fg(theme::ERROR())
                .add_modifier(Modifier::BOLD),
        )));
    }
    frame.render_widget(Paragraph::new(title_lines), rows[0]);
    // While composing a chat line, reserve the bottom row for the say prompt.
    // The body above it keeps drawing whatever panel is open, map included, so
    // pressing `'` never swaps the view out from under you.
    let chat = state.chat_text();
    let (body, prompt) = match chat.is_some() {
        true => {
            let split =
                Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(rows[1]);
            (split[0], Some(split[1]))
        }
        false => (rows[1], None),
    };
    // Last frame's clickable chips are stale now; a bar that isn't drawn this
    // frame (map open, cramped view) must leave nothing behind to click.
    state.clear_combat_hits();
    // Tell the state what this frame could actually draw, so `m` never cycles
    // into a page this terminal has to fall back out of.
    state.set_lands_available(lands_fit(body));
    let land_page = state.map_open() && state.map_mode() == MapMode::Lands;
    if view.classed && land_page && lands_fit(body) {
        draw_land_map(frame, body, state, &view);
    } else if view.classed && state.map_open() && !land_page && map_fits(body) {
        draw_world_map(frame, body, state, &view);
    } else {
        // A classed adventurer gets a clickable action bar on the bottom row -
        // attack, ability slots, quaff, flee - so a fight can be run with the
        // mouse without leaving the view. Keyboard keeps working unchanged.
        let game_area = if view.classed && body.height >= 6 {
            let split = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(body);
            draw_action_bar(frame, split[1], state, &view);
            split[0]
        } else {
            body
        };
        // Below the graphical map's minimum, Panel::Map falls back to the text
        // atlas in the side panel (see `draw_side`).
        draw_game(frame, game_area, state, usernames);
    }
    if let (Some(text), Some(prompt)) = (chat, prompt) {
        // Reflect the channel a `/z` or `/w` marker will send to before it's
        // sent, so the scope is never a surprise.
        let label = if text.starts_with("/zone ") || text.starts_with("/z ") {
            "Say to zone: "
        } else if text.starts_with("/world ") || text.starts_with("/w ") {
            "Say to Lateania: "
        } else {
            "Say: "
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    label,
                    Style::default()
                        .fg(theme::AMBER_GLOW())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{text}\u{2588}"),
                    Style::default().fg(theme::TEXT_BRIGHT()),
                ),
                Span::styled(
                    "   (Enter send · Esc cancel)",
                    Style::default().fg(theme::TEXT_DIM()),
                ),
            ])),
            prompt,
        );
    }
}

// ---- Does the map fit, and the small shared name/style lookups -----------

/// The ways up and down. Deliberately loud (the brightest thing on the map
/// after `@`): these are the exits the flat grid cannot draw as corridors, and
/// in a world where every zone chains to the next one by a stair they are what
/// a lost player is looking for.
fn stair_style() -> Style {
    Style::default()
        .fg(theme::SUCCESS())
        .add_modifier(Modifier::BOLD)
}

/// The land map needs enough width to hold a branch chip, the trunk, and
/// another branch chip side by side; narrower than that and the picture clips
/// instead of informing, so the text atlas serves instead. Height is looser,
/// since `[` / `]` scroll the map.
fn lands_fit(area: Rect) -> bool {
    area.width >= 76 && area.height >= 12
}

/// The smallest area the graphical world map is worth drawing in.
///
/// This used to be 50x14, and everything smaller was handed the text atlas in
/// the side rail instead - which meant a phone-sized terminal never saw the
/// map at all. The one view that answers "where am I" was the one view a phone
/// could not open. What did not fit was never the map: it was the seven rows of
/// header, inspector, controls and legends stacked around it, and `MapChrome`
/// now sheds those instead (see there). What is left is a floor the map body
/// itself needs - a handful of rows of world and enough columns for the
/// player's own neighbourhood to read.
fn map_fits(area: Rect) -> bool {
    area.width >= MAP_MIN_W && area.height >= MAP_MIN_H
}

const MAP_MIN_W: u16 = 32;
const MAP_MIN_H: u16 = 10;

/// Which of the map's chrome rows this terminal can afford.
///
/// The rows are shed in reverse order of what they are worth to someone who is
/// lost: the terrain key first (the biome colours are already on the map), then
/// the marker legend, then the symbol legend, then the inspector's second row.
/// The header, the crosshair's first row and the controls line stay to the
/// floor, because between them they say where you are, what you are pointing
/// at, and how to move the view.
///
/// Width sheds the legends too, and sheds them first: they are single
/// unwrapped lines around 90 columns long, so on a narrow terminal they were
/// never a legend at all - they were a sentence clipped mid-word at the edge,
/// costing a row of map to say "known, el".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MapChrome {
    /// Rows the crosshair inspector gets: 2 (region + what is there), 1
    /// (region only), or 0 on the very smallest map.
    inspector: u16,
    controls: bool,
    symbols: bool,
    markers: bool,
    terrain: bool,
}

/// The width a full legend line needs before it stops being clipped mid-word.
const MAP_LEGEND_W: u16 = 92;

impl MapChrome {
    fn for_area(area: Rect) -> Self {
        let legend_w = area.width >= MAP_LEGEND_W;
        Self {
            inspector: match area.height {
                0..=11 => 1,
                _ => 2,
            },
            controls: area.height >= 11,
            symbols: legend_w && area.height >= 16,
            markers: legend_w && area.height >= 18,
            // The terrain key only names the biomes actually in view, so it is
            // short enough to survive a narrower terminal than the legends.
            terrain: area.width >= 60 && area.height >= 20,
        }
    }

    /// The layout constraints for everything below the map body, in order.
    fn footer_constraints(&self) -> Vec<Constraint> {
        let mut rows = Vec::new();
        if self.inspector > 0 {
            rows.push(Constraint::Length(self.inspector));
        }
        for present in [self.controls, self.symbols, self.markers, self.terrain] {
            if present {
                rows.push(Constraint::Length(1));
            }
        }
        rows
    }
}

/// The map's control line, sized to the terminal. The full line is 78 columns,
/// so on anything narrow it used to be clipped to "wasd pan · <> level · x mark
/// destina" - the keys that close the map and mark a destination, the two a lost
/// player most needs, were the ones that fell off the end.
fn map_controls_line(width: u16) -> &'static str {
    // Each arm's own width is the floor of the band above it, so the line
    // always fits the columns it is painted into rather than being chopped.
    match width {
        0..=37 => "wasd pan · x mark · m close",
        38..=66 => "wasd pan · x mark · q quests · m close",
        67..=78 => "wasd pan · <> level · x mark · q quests · Enter re-centre · m close",
        _ => "wasd pan · <> level · x mark destination · q quests · Enter re-centre · m close",
    }
}

/// Per-biome map glyph and colour for the overhead world map.
/// A short land name for the map overlay: the atlas carries full titles like
/// "Embergate & the King's Road", but a map label wants just "Embergate". Drop
/// any " & ..." or ", ..." tail so the name fits a clear run and reads at a glance.
fn land_label(name: &str) -> &str {
    let name = name.split(" & ").next().unwrap_or(name);
    name.split(", ").next().unwrap_or(name)
}

// ---- The overhead field: biomes, ground tiles, POI arrows (5.1) ----------

fn biome_style(biome: super::world::Biome) -> (char, Color) {
    use super::world::Biome;
    match biome {
        Biome::Heartland => ('"', Color::Rgb(120, 180, 90)),
        Biome::Plains => ('.', Color::Rgb(150, 160, 80)),
        Biome::Urban => ('#', Color::Rgb(175, 175, 175)),
        Biome::Forest => ('\u{2663}', Color::Rgb(60, 145, 70)), // ♣
        Biome::Water => ('\u{2248}', Color::Rgb(70, 120, 205)), // ≈
        Biome::Islands => ('~', Color::Rgb(90, 175, 185)),
        Biome::Ash => ('%', Color::Rgb(165, 85, 70)),
        Biome::Cavern => ('\u{00b7}', Color::Rgb(125, 115, 135)), // ·
        Biome::Badlands => (':', Color::Rgb(165, 125, 80)),
    }
}

/// One artistic ground tile for the live field: the biome's terrain drawn as a
/// scatter of little glyphs (grass tufts, trees, waves, ash, rock) picked by a
/// hash of the world cell, so the ground looks hand-strewn and varied yet stays
/// perfectly stable as you walk (no shimmer) instead of a flat wash of one char.
/// `(hx, hy)` are stable per-world-location coordinates. Every biome has its own
/// palette so a forest, a lake, an ash waste, and a cavern each read at a glance.
fn field_ground(biome: super::world::Biome, hx: i32, hy: i32) -> (char, Color) {
    use super::world::Biome;
    // Cheap stable spatial hash -> a bucket 0..15. Low buckets are the rarer
    // decorations (trees/flowers/rocks); the rest is base ground, so features
    // sprinkle in sparsely.
    let h = (hx.wrapping_mul(73_856_093) ^ hy.wrapping_mul(19_349_663)) as u32;
    let b = (h >> 4) % 16;
    match biome {
        // Home fields: lush grass with the odd wildflower.
        Biome::Heartland => match b {
            0 => ('*', Color::Rgb(230, 205, 90)),
            1 => ('\u{2740}', Color::Rgb(225, 130, 150)), // ❀
            2..=3 => ('"', Color::Rgb(105, 165, 80)),
            4..=6 => (',', Color::Rgb(95, 155, 72)),
            _ => ('.', Color::Rgb(80, 140, 66)),
        },
        // Open overworld: drier, wind-combed grass.
        Biome::Plains => match b {
            0 => ('\'', Color::Rgb(180, 175, 95)),
            1..=2 => ('"', Color::Rgb(160, 160, 85)),
            3..=5 => (',', Color::Rgb(150, 150, 78)),
            _ => ('.', Color::Rgb(135, 138, 72)),
        },
        // Capitals and villages: flagstone and cobble.
        Biome::Urban => match b {
            0 => ('#', Color::Rgb(150, 150, 155)),
            1 => ('=', Color::Rgb(140, 140, 145)),
            2..=3 => (':', Color::Rgb(120, 120, 128)),
            _ => ('.', Color::Rgb(105, 105, 114)),
        },
        // Greenwood: dark undergrowth studded with trees.
        Biome::Forest => match b {
            0 => ('\u{2660}', Color::Rgb(40, 105, 48)), // ♠ tree
            1 => ('\u{2663}', Color::Rgb(46, 120, 55)), // ♣ tree
            2..=3 => ('"', Color::Rgb(58, 118, 62)),
            4..=6 => (',', Color::Rgb(52, 105, 58)),
            _ => ('.', Color::Rgb(44, 92, 52)),
        },
        // Open water: rolling waves.
        Biome::Water => match b {
            0..=1 => ('\u{2248}', Color::Rgb(90, 140, 220)), // ≈
            2..=4 => ('~', Color::Rgb(70, 120, 205)),
            _ => ('~', Color::Rgb(58, 105, 190)),
        },
        // Archipelago: pale shore and shallows.
        Biome::Islands => match b {
            0 => ('\u{2248}', Color::Rgb(95, 180, 190)),
            1..=2 => ('~', Color::Rgb(85, 170, 182)),
            3..=4 => ('.', Color::Rgb(205, 195, 150)), // sand
            _ => (',', Color::Rgb(120, 175, 165)),
        },
        // Ashen Reach: cinders and cooling embers.
        Biome::Ash => match b {
            0 => ('%', Color::Rgb(180, 90, 70)),
            1 => ('*', Color::Rgb(205, 110, 60)), // ember
            2..=3 => ('"', Color::Rgb(120, 80, 78)),
            4..=5 => (',', Color::Rgb(105, 72, 70)),
            _ => ('.', Color::Rgb(88, 66, 66)),
        },
        // Caverns: rock floor with the odd boulder.
        Biome::Cavern => match b {
            0 => ('\u{2593}', Color::Rgb(120, 112, 128)), // ▓ rock
            1 => ('#', Color::Rgb(108, 100, 118)),
            2..=3 => ('\u{00b7}', Color::Rgb(130, 122, 140)), // ·
            4..=5 => (',', Color::Rgb(102, 96, 112)),
            _ => ('.', Color::Rgb(88, 84, 98)),
        },
        // Badlands: cracked hardpan and scrub.
        Biome::Badlands => match b {
            0 => ('\u{25b2}', Color::Rgb(150, 110, 70)), // ▲ mesa
            1..=2 => (':', Color::Rgb(165, 125, 80)),
            3..=5 => (',', Color::Rgb(150, 115, 74)),
            _ => ('.', Color::Rgb(135, 104, 68)),
        },
    }
}

/// Whether a room offers a service worth marking on the field: a merchant, a
/// crafting station, or an actionable feature (stable, portal, bank, quest
/// board, housing clerk, fountain). All static, so this costs no snapshot data.
fn is_service_room(id: u32) -> bool {
    use super::world::FeatureKind;
    super::items::shop_at(id).is_some()
        || !super::world::craft_stations_at(id).is_empty()
        || super::world::features_at(id).iter().any(|f| {
            matches!(
                f.kind,
                FeatureKind::Fountain
                    | FeatureKind::Bank
                    | FeatureKind::Board
                    | FeatureKind::Stable
                    | FeatureKind::Housing
                    | FeatureKind::Portal
                    | FeatureKind::CraftStation(_)
            )
        })
}

/// Pull off-screen POI arrows in from the widget border so they hug the
/// explored cluster instead of floating at the panel's far edge, where nothing
/// ties them to the map they annotate. Arrows collapsing onto the same cell
/// keep boss priority. Atlas only: the live field draws no POI arrows, so a
/// glyph next to `@` can never masquerade as a movement affordance.
fn hug_poi_arrows(
    arrows: Vec<super::worldmap::MapArrow>,
    canvas: &[Vec<super::worldmap::Tile>],
) -> Vec<super::worldmap::MapArrow> {
    use super::worldmap::{MapArrow, Tile};

    let mut bounds: Option<(usize, usize, usize, usize)> = None;
    for (r, row) in canvas.iter().enumerate() {
        for (c, tile) in row.iter().enumerate() {
            if !matches!(tile, Tile::Empty) {
                let b = bounds.get_or_insert((r, c, r, c));
                b.0 = b.0.min(r);
                b.1 = b.1.min(c);
                b.2 = b.2.max(r);
                b.3 = b.3.max(c);
            }
        }
    }
    let Some((r0, c0, r1, c1)) = bounds else {
        return arrows;
    };
    let max_r = canvas.len() - 1;
    let max_c = canvas[0].len() - 1;
    let mut hugged: std::collections::BTreeMap<(usize, usize), MapArrow> =
        std::collections::BTreeMap::new();
    for a in arrows {
        let row = a.row.clamp(r0.saturating_sub(1), (r1 + 1).min(max_r));
        let col = a.col.clamp(c0.saturating_sub(1), (c1 + 1).min(max_c));
        let moved = MapArrow { row, col, ..a };
        let e = hugged.entry((row, col)).or_insert(moved);
        if a.boss && !e.boss {
            *e = moved;
        }
    }
    hugged.into_values().collect()
}

/// The live play field: a scrolling, biome-coloured top-down view kept centred
/// on the player, so ordinary movement walks you across a rendered world (the
/// terrain and paths ahead are drawn as you go, fog lifting as you explore).
/// Unlike the overhead map (`m`), this never pans - it just follows you. Lines
/// on the field mean exactly one thing, walkable path: bright stubs are the
/// current room's exits, faint stubs are paths running on into fog. POI
/// direction arrows belong to the atlas only.
/// The map header's layer caption. `z` counts layers, and for the underground
/// zones it still reads as literal depth, but ever since each descent zone
/// took its own level a couple of zones sit below z 0 while being open sky in
/// the fiction. Those name their own layer while the player is looking at the
/// layer they stand on; every other view keeps the plain depth wording.
fn level_label(player_room: RoomId, viewed_z: i32, player_z: i32) -> String {
    if viewed_z == player_z
        && let Some(label) = match super::worldmap::zone_of(player_room) {
            Some("Frostspire Ascent") => Some("mountainside"),
            Some("the Saltwind Wharves") => Some("the waterline"),
            _ => None,
        }
    {
        return label.to_string();
    }
    match viewed_z {
        0 => "surface".to_string(),
        z if z < 0 => format!("underground {}", -z),
        z => format!("above {z}"),
    }
}

fn draw_field(frame: &mut Frame, area: Rect, view: &PlayerView) {
    use super::world::region_atlas_entry;
    use super::worldmap::{Tile, map_canvas, poi, world_coords};

    if area.height < 3 || area.width < 8 {
        return;
    }
    let coords = world_coords();
    let Some(player_room) = view.room else {
        return;
    };
    let Some(&center) = coords.get(&player_room) else {
        frame.render_widget(
            Paragraph::new("No field view for this place.")
                .style(Style::default().fg(theme::TEXT_DIM())),
            area,
        );
        return;
    };

    let rows = Layout::vertical([
        Constraint::Length(1), // where-am-i header
        Constraint::Min(1),    // the field itself
        Constraint::Length(1), // colour key
    ])
    .split(area);

    // Header: the land you stand in and the depth, so the field is grounded.
    let (region_name, tier) = region_atlas_entry(player_room).unwrap_or(("The wilds", ""));
    let depth = level_label(player_room, center.z, center.z);
    let mut header = vec![Span::styled(
        region_name.to_string(),
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD),
    )];
    if !tier.is_empty() {
        header.push(Span::styled(
            format!("  ·  {tier}"),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    header.push(Span::styled(
        format!("  ·  {depth}"),
        Style::default().fg(theme::TEXT_FAINT()),
    ));
    frame.render_widget(Paragraph::new(Line::from(header)), rows[0]);

    let body = rows[1];
    let cols = body.width as i32;
    let height = body.height as i32;
    // Colour-coded detail on what's around you. Landmarks (boss/tame) come from
    // the static atlas; a red dagger marks a room holding a live foe (from the
    // snapshot's nearby list); a gem marks a harvestable resource room (static).
    let foes: std::collections::HashSet<u32> = view.nearby_foes.iter().copied().collect();
    let players: std::collections::HashSet<u32> = view.nearby_players.iter().copied().collect();
    let canvas = map_canvas(coords, center, cols, height, &view.visited, player_room);

    // The player token turns hostile-red in a fight, so a glance at the field
    // tells you combat is on even with your eyes off the log.
    let player_style = if view.mobs.is_empty() {
        Style::default()
            .fg(Color::Rgb(250, 240, 140))
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(Color::Rgb(240, 120, 90))
            .add_modifier(Modifier::BOLD)
    };
    let boss_style = Style::default()
        .fg(Color::Rgb(250, 210, 90))
        .add_modifier(Modifier::BOLD);
    let tame_style = Style::default()
        .fg(Color::Rgb(230, 140, 160))
        .add_modifier(Modifier::BOLD);
    // Trodden paths between rooms: a warm dim so they read as trails cut through
    // the terrain rather than fences.
    let path_style = Style::default().fg(Color::Rgb(150, 120, 82));

    // Rooms sit on even offsets from the centre cell; `(hx, hy)` are stable
    // world coordinates for a cell so the ground scatter never shimmers as you
    // walk. The biome under an empty cell is borrowed from the nearest drawn
    // room, so terrain fills the gaps between explored rooms and simply stops at
    // the fog (an empty cell with no drawn neighbour stays black = unknown).
    let (cx, cy) = (cols / 2, height / 2);
    let biome_at = |sr: i32, sc: i32| -> Option<super::world::Biome> {
        for (dr, dc) in [
            (0, 0),
            (-1, 0),
            (1, 0),
            (0, -1),
            (0, 1),
            (-1, -1),
            (-1, 1),
            (1, -1),
            (1, 1),
        ] {
            let (r, c) = (sr + dr, sc + dc);
            if r >= 0
                && r < height
                && c >= 0
                && c < cols
                && let Tile::Room(id) = canvas[r as usize][c as usize]
            {
                return Some(super::world::biome_of(id));
            }
        }
        None
    };
    // `is_room` distinguishes the two ways a biome fills a cell. Open biomes
    // scatter the same ground everywhere. Enclosed ones (caverns) instead carve:
    // rooms and corridors are open floor, and the space between is solid rock, so
    // an interior reads as passages cut through stone rather than an open plain.
    let terrain_cell = |sr: i32, sc: i32, biome: super::world::Biome, is_room: bool| {
        let hx = 2 * center.x + (sc - cx);
        let hy = 2 * center.y + (sr - cy);
        if matches!(biome, super::world::Biome::Cavern) {
            let h = (hx.wrapping_mul(73_856_093) ^ hy.wrapping_mul(19_349_663)) as u32;
            if is_room {
                let ch = if h.is_multiple_of(6) { ',' } else { '.' };
                (
                    ch.to_string(),
                    Style::default().fg(Color::Rgb(140, 132, 152)),
                )
            } else {
                // Mostly medium rock with the odd darker/heavier block for texture.
                let ch = match h % 8 {
                    0 => '\u{2588}',     // █
                    1 | 2 => '\u{2593}', // ▓
                    _ => '\u{2592}',     // ▒
                };
                (ch.to_string(), Style::default().fg(Color::Rgb(78, 73, 92)))
            }
        } else {
            let (g, color) = field_ground(biome, hx, hy);
            (g.to_string(), Style::default().fg(color))
        }
    };

    let foe_style = Style::default()
        .fg(Color::Rgb(235, 90, 80))
        .add_modifier(Modifier::BOLD);
    let player_near_style = Style::default()
        .fg(Color::Rgb(120, 210, 235))
        .add_modifier(Modifier::BOLD);
    let service_style = Style::default().fg(Color::Rgb(180, 160, 235));
    let node_style = Style::default().fg(Color::Rgb(210, 180, 110));
    let room_glyph = |id: u32, sr: i32, sc: i32| -> (String, Style) {
        if foes.contains(&id) {
            return ("\u{2020}".to_string(), foe_style); // † a foe lairs here
        }
        if let Some(p) = poi(id) {
            if p.boss.is_some() {
                return ("\u{2605}".to_string(), boss_style); // ★
            }
            if p.tameable.is_some() {
                return ("\u{2665}".to_string(), tame_style); // ♥
            }
        }
        if players.contains(&id) {
            return ("\u{263a}".to_string(), player_near_style); // ☺ another adventurer
        }
        if is_service_room(id) {
            return ("\u{2302}".to_string(), service_style); // ⌂ a shop / stable / station / portal
        }
        if !super::world::nodes_at(id).is_empty() {
            return ("\u{2666}".to_string(), node_style); // ♦ a resource to gather
        }
        terrain_cell(sr, sc, super::world::biome_of(id), true)
    };

    let mut cells: Vec<Vec<(String, Style)>> =
        vec![
            vec![(" ".to_string(), Style::default()); cols.max(0) as usize];
            height.max(0) as usize
        ];
    for sr in 0..height {
        for sc in 0..cols {
            let cell = match canvas[sr as usize][sc as usize] {
                // Rooms melt into the terrain (the ground scatter carries the
                // biome); only the player, landmarks, and paths stand proud.
                Tile::Room(id) if id == player_room => ("@".to_string(), player_style),
                Tile::Room(id) => room_glyph(id, sr, sc),
                Tile::LinkH => ("\u{2500}".to_string(), path_style),
                Tile::LinkV => ("\u{2502}".to_string(), path_style),
                // A path running off into the unknown: a faint arrow pointing the
                // way, so a discovered spot never looks stranded (no spoiler).
                Tile::Hint(ch) => (ch.to_string(), Style::default().fg(theme::TEXT_FAINT())),
                // Same idea, but the far side is already explored (a
                // non-Euclidean jump, not the edge of the map) - brighter, so
                // it doesn't read as "nothing more to find here".
                Tile::HintKnown(ch) => (ch.to_string(), Style::default().fg(theme::AMBER_DIM())),
                // A way up or down out of the room beside it. No flat
                // direction can carry this, and it is usually the way onward.
                Tile::Stair(ch) => (ch.to_string(), stair_style()),
                Tile::Empty => match biome_at(sr, sc) {
                    Some(biome) => terrain_cell(sr, sc, biome, false),
                    None => (" ".to_string(), Style::default()),
                },
            };
            cells[sr as usize][sc as usize] = cell;
        }
    }

    // The current room's exits, drawn as bright stubs from the player's own
    // cell: the map answers "which way can I walk right now" exactly where the
    // eye rests. Exits come from the room data, not the geometry, so the stub
    // is an honest affordance even where the hand-authored graph scatters a
    // destination somewhere non-adjacent. Rooms sit on even offsets, so the
    // stub cell (one step out) can never hold another room's glyph.
    let exit_style = Style::default()
        .fg(Color::Rgb(220, 185, 120))
        .add_modifier(Modifier::BOLD);
    for (dir, _) in &view.exits {
        let (dx, dy, glyph) = match dir {
            Dir::East => (1, 0, '\u{2500}'),
            Dir::West => (-1, 0, '\u{2500}'),
            Dir::North => (0, -1, '\u{2502}'),
            Dir::South => (0, 1, '\u{2502}'),
            // The side panel's exits line carries the stairs.
            Dir::Up | Dir::Down => continue,
        };
        let (sc, sr) = (cx + dx, cy + dy);
        if (0..cols).contains(&sc)
            && (0..height).contains(&sr)
            && let Some(cell) = cells
                .get_mut(sr as usize)
                .and_then(|r| r.get_mut(sc as usize))
        {
            *cell = (glyph.to_string(), exit_style);
        }
    }

    let lines: Vec<Line> = cells
        .into_iter()
        .map(|row| {
            Line::from(
                row.into_iter()
                    .map(|(ch, style)| Span::styled(ch, style))
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), body);

    // Colour key, so every marker on the field reads at a glance.
    let dim = Style::default().fg(theme::TEXT_DIM());
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("@", player_style),
            Span::styled(" you ", dim),
            Span::styled("\u{2500}\u{2502}", path_style),
            Span::styled(" path ", dim),
            Span::styled("\u{21d3}\u{21d1}", stair_style()),
            Span::styled(" stair ", dim),
            Span::styled("\u{2020}", foe_style),
            Span::styled(" foe ", dim),
            Span::styled("\u{263a}", player_near_style),
            Span::styled(" player ", dim),
            Span::styled("\u{2605}", boss_style),
            Span::styled(" boss ", dim),
            Span::styled("\u{2665}", tame_style),
            Span::styled(" tame ", dim),
            Span::styled("\u{2302}", service_style),
            Span::styled(" town ", dim),
            Span::styled("\u{2666}", node_style),
            Span::styled(" node ", dim),
            Span::styled("\u{2500}", exit_style),
            Span::styled(" way out ", dim),
            Span::styled("\u{2500}", Style::default().fg(theme::TEXT_FAINT())),
            Span::styled(" unexplored", dim),
        ])),
        rows[2],
    );
}

/// The land map: an atlas of the whole realm, every country drawn where it
/// sits and every road between two countries drawn as a line. The two great
/// hubs are walled keeps; everything else is a name on the road that reaches
/// it. It carries no bosses, no gate titles, and no level bands on purpose.
/// The question it answers is "how do I get there"; what *opens* a road is
/// still something you find by walking to it. Every land is named, walked or
/// not, because the name of a country was never the secret.
fn draw_land_map(frame: &mut Frame, area: Rect, state: &State, view: &PlayerView) {
    let mut lines = land_map_lines(&view.atlas, area.height as usize);
    // `[` / `]` scroll, clamped so the last line can reach the top but no
    // further; on a short terminal the map is taller than the body.
    let max_off = lines.len().saturating_sub(area.height as usize);
    let off = state.list_scroll().min(max_off);
    state.set_list_scroll(off);
    frame.render_widget(Paragraph::new(lines.split_off(off)), area);
}

// ---- The atlas: where each land sits, and which roads join them ----------
//
// The picture is *drawn*, not derived. A map that lays itself out is a graph,
// and a graph of an eighteen-country world with two ten-road hubs reads as a
// list however it is arranged - three earlier attempts (an indented tree, a
// fan of unattached columns, a trunk with spurs) all failed the same way. So
// the placement below is cartography: hand-set rows and columns, chosen so
// gentle country sits north of the road and the deep dark sits south of it.
//
// What is *not* authored is which lands touch. `ROADS` may only name pairs
// that `worldmap::land_links` derives from the room graph, and must name all
// of them; `the_atlas_draws_every_road_in_the_world_and_invents_none` fails
// the build if the map and the world ever disagree. Adding a country means
// finding it a place here - which is the point, since a map nobody placed it
// on is a map that quietly lost it.

/// The canvas the atlas is drawn on. Wider than this and `lands_fit` would
/// have to refuse more terminals; the layout is built to end at column 74.
const MAP_W: usize = 76;
const MAP_H: usize = 13;

/// How a land's label meets the road that reaches it. Left-hand lands *end* at
/// their anchor and grow leftward, right-hand ones *start* at it, so a depth
/// counter gaining a digit pushes the name away from the road instead of
/// shoving the road it is attached to.
#[derive(Clone, Copy)]
enum At {
    Ends(usize),
    Starts(usize),
    Centered(usize),
}

/// A land drawn as a name on the map.
struct Place {
    region: &'static str,
    row: usize,
    at: At,
}

/// A land drawn as a walled keep: the two hubs every road runs through.
struct Keep {
    region: &'static str,
    top: usize,
    left: usize,
    bottom: usize,
    right: usize,
}

#[derive(Clone, Copy)]
enum Leg {
    Up(usize),
    Down(usize),
    Left(usize),
    Right(usize),
}

/// One road: the two lands it joins, where it leaves, and how it runs. It
/// starts on the keep wall (or beside a name) and each leg carries it that
/// many cells; corners and wall junctions are worked out from the turns.
struct Road {
    a: &'static str,
    b: &'static str,
    from: (usize, usize),
    legs: &'static [Leg],
}

const KEEPS: &[Keep] = &[
    Keep {
        region: "The Overworld & Capitals",
        top: 3,
        left: 20,
        bottom: 5,
        right: 42,
    },
    Keep {
        region: "Embergate & the King's Road",
        top: 3,
        left: 56,
        bottom: 5,
        right: 74,
    },
];

const PLACES: &[Place] = &[
    // North of the road: the countries a living adventurer walks to.
    Place {
        region: "Aelunor, the Faewood",
        row: 0,
        at: At::Ends(16),
    },
    Place {
        region: "Silvael",
        row: 0,
        at: At::Ends(27),
    },
    Place {
        region: "The Wildbound Waste",
        row: 1,
        at: At::Ends(23),
    },
    Place {
        region: "Wayfarer's Hollow",
        row: 1,
        at: At::Ends(63),
    },
    Place {
        region: "The Sunderlakes",
        row: 2,
        at: At::Ends(19),
    },
    // Between the two keeps, because it opens off both of them.
    Place {
        region: "City Districts",
        row: 2,
        at: At::Starts(43),
    },
    Place {
        region: "Broceliande, the Greenwood",
        row: 4,
        at: At::Ends(17),
    },
    // Chained outward from Broceliande, same as Aelunor off Silvael.
    Place {
        region: "Thornveil Falls",
        row: 5,
        at: At::Ends(17),
    },
    // South of the road: the dark, and the way down into it.
    Place {
        region: "The Sunken Catacombs",
        row: 6,
        at: At::Ends(19),
    },
    Place {
        region: "The Frontier",
        row: 6,
        at: At::Ends(55),
    },
    Place {
        region: "Thornwood Hollows",
        row: 7,
        at: At::Ends(23),
    },
    Place {
        region: "Hearthward Close",
        row: 7,
        at: At::Ends(63),
    },
    Place {
        region: "The Drowned Caverns",
        row: 8,
        at: At::Ends(27),
    },
    Place {
        region: "The Sundered Reaches",
        row: 9,
        at: At::Centered(39),
    },
    Place {
        region: "Kaelmyr, the Ashen Reach",
        row: 12,
        at: At::Centered(39),
    },
];

const ROADS: &[Road] = &[
    Road {
        a: "The Overworld & Capitals",
        b: "Embergate & the King's Road",
        from: (4, 42),
        legs: &[Leg::Right(14)],
    },
    Road {
        a: "The Overworld & Capitals",
        b: "The Sunderlakes",
        from: (3, 23),
        legs: &[Leg::Up(1), Leg::Left(2)],
    },
    Road {
        a: "The Overworld & Capitals",
        b: "The Wildbound Waste",
        from: (3, 27),
        legs: &[Leg::Up(2), Leg::Left(2)],
    },
    Road {
        a: "The Overworld & Capitals",
        b: "Silvael",
        from: (3, 31),
        legs: &[Leg::Up(3), Leg::Left(2)],
    },
    Road {
        a: "Silvael",
        b: "Aelunor, the Faewood",
        from: (0, 18),
        legs: &[Leg::Right(1)],
    },
    Road {
        a: "The Overworld & Capitals",
        b: "City Districts",
        from: (3, 39),
        legs: &[Leg::Up(1), Leg::Right(2)],
    },
    Road {
        a: "Embergate & the King's Road",
        b: "City Districts",
        from: (3, 60),
        legs: &[Leg::Up(1), Leg::Left(2)],
    },
    Road {
        a: "Embergate & the King's Road",
        b: "Wayfarer's Hollow",
        from: (3, 67),
        legs: &[Leg::Up(2), Leg::Left(2)],
    },
    Road {
        a: "The Overworld & Capitals",
        b: "Broceliande, the Greenwood",
        from: (4, 20),
        legs: &[Leg::Left(1)],
    },
    Road {
        a: "Broceliande, the Greenwood",
        b: "Thornveil Falls",
        from: (4, 18),
        legs: &[Leg::Down(1)],
    },
    Road {
        a: "The Overworld & Capitals",
        b: "The Sunken Catacombs",
        from: (5, 23),
        legs: &[Leg::Down(1), Leg::Left(2)],
    },
    Road {
        a: "The Overworld & Capitals",
        b: "Thornwood Hollows",
        from: (5, 27),
        legs: &[Leg::Down(2), Leg::Left(2)],
    },
    Road {
        a: "The Overworld & Capitals",
        b: "The Drowned Caverns",
        from: (5, 31),
        legs: &[Leg::Down(3), Leg::Left(2)],
    },
    Road {
        a: "The Overworld & Capitals",
        b: "The Sundered Reaches",
        from: (5, 39),
        legs: &[Leg::Down(3)],
    },
    Road {
        a: "The Sundered Reaches",
        b: "Kaelmyr, the Ashen Reach",
        from: (10, 39),
        legs: &[Leg::Down(1)],
    },
    Road {
        a: "Embergate & the King's Road",
        b: "The Frontier",
        from: (5, 59),
        legs: &[Leg::Down(1), Leg::Left(2)],
    },
    Road {
        a: "Embergate & the King's Road",
        b: "Hearthward Close",
        from: (5, 67),
        legs: &[Leg::Down(2), Leg::Left(2)],
    },
];

const UP: u8 = 1;
const DOWN: u8 = 2;
const LEFT: u8 = 4;
const RIGHT: u8 = 8;

/// A styled character grid. Names and walls go down first, roads after, so a
/// road arriving at a keep can read the wall it lands on and turn it into the
/// right junction rather than punching a hole through it.
struct Canvas {
    cells: Vec<(char, Style)>,
}

impl Canvas {
    fn new() -> Self {
        Canvas {
            cells: vec![(' ', Style::default()); MAP_W * MAP_H],
        }
    }

    fn ch(&self, row: usize, col: usize) -> char {
        self.cells[row * MAP_W + col].0
    }

    fn set(&mut self, row: usize, col: usize, ch: char, style: Style) {
        if row < MAP_H && col < MAP_W {
            self.cells[row * MAP_W + col] = (ch, style);
        }
    }

    fn write(&mut self, row: usize, col: usize, text: &str, style: Style) {
        for (i, ch) in text.chars().enumerate() {
            self.set(row, col + i, ch, style);
        }
    }

    /// A land's label: its short name, then how many of its zones you have
    /// walked. Depth is in zones rather than rooms because a country can be
    /// three zones deep on 2% of its rooms, and depth is the number that says
    /// how far in you are.
    fn label(&mut self, region: &str, row: usize, at: At, progress: &Progress) -> usize {
        let name = land_chip_name(region);
        let entered = progress.get(region).copied().and_then(|p| p.chain);
        let depth = entered.map(|(walked, zones)| format!("  {walked}/{zones}"));
        let width = name.chars().count() + depth.as_ref().map_or(0, |d| d.chars().count());
        let col = match at {
            At::Ends(end) => (end + 1).saturating_sub(width),
            At::Starts(start) => start,
            At::Centered(mid) => mid.saturating_sub(width / 2),
        };
        self.write(row, col, &name, land_style(progress.get(region).copied()));
        if let Some(depth) = depth {
            let dim = match entered {
                Some((walked, _)) if walked > 0 => theme::AMBER_DIM(),
                _ => theme::TEXT_DIM(),
            };
            self.write(
                row,
                col + name.chars().count(),
                &depth,
                Style::default().fg(dim),
            );
        }
        col
    }

    fn keep(&mut self, keep: &Keep, progress: &Progress) {
        let wall = Style::default().fg(theme::BORDER());
        for col in keep.left + 1..keep.right {
            self.set(keep.top, col, '\u{2500}', wall);
            self.set(keep.bottom, col, '\u{2500}', wall);
        }
        for row in keep.top + 1..keep.bottom {
            self.set(row, keep.left, '\u{2502}', wall);
            self.set(row, keep.right, '\u{2502}', wall);
        }
        self.set(keep.top, keep.left, '\u{256D}', wall);
        self.set(keep.top, keep.right, '\u{256E}', wall);
        self.set(keep.bottom, keep.left, '\u{2570}', wall);
        self.set(keep.bottom, keep.right, '\u{256F}', wall);
        let name = land_chip_name(keep.region).to_uppercase();
        let mid = (keep.left + keep.right) / 2;
        let col = mid - name.chars().count() / 2;
        self.write(
            (keep.top + keep.bottom) / 2,
            col,
            &name,
            land_style(progress.get(keep.region).copied()).add_modifier(Modifier::BOLD),
        );
    }

    /// A road runs in the wall colour once both the lands it joins are walked,
    /// and faint until then, so the picture separates the roads you know from
    /// the ones you have only been told about.
    fn road(&mut self, road: &Road, progress: &Progress) {
        let known = |region: &str| {
            progress
                .get(region)
                .is_some_and(|p: &&super::world::RegionProgress| p.explored > 0)
        };
        let ink = match known(road.a) && known(road.b) {
            true => theme::BORDER(),
            false => theme::TEXT_FAINT(),
        };
        let mut cells: Vec<((usize, usize), u8)> = vec![(road.from, 0)];
        let (mut row, mut col) = road.from;
        for leg in road.legs {
            let (out, back, n) = match *leg {
                Leg::Up(n) => (UP, DOWN, n),
                Leg::Down(n) => (DOWN, UP, n),
                Leg::Left(n) => (LEFT, RIGHT, n),
                Leg::Right(n) => (RIGHT, LEFT, n),
            };
            for _ in 0..n {
                if let Some(last) = cells.last_mut() {
                    last.1 |= out;
                }
                match *leg {
                    Leg::Up(_) => row -= 1,
                    Leg::Down(_) => row += 1,
                    Leg::Left(_) => col -= 1,
                    Leg::Right(_) => col += 1,
                }
                cells.push(((row, col), back));
            }
        }
        for ((row, col), dirs) in cells {
            self.stroke(row, col, dirs, ink);
        }
    }

    /// One cell of road. A cell that lands on a keep wall becomes the junction
    /// where that road leaves the city; anywhere else it is a straight run or
    /// the corner the two directions make.
    fn stroke(&mut self, row: usize, col: usize, dirs: u8, ink: Color) {
        let ch = match self.ch(row, col) {
            '\u{2500}' if dirs & DOWN != 0 => '\u{252C}', // ─ + down  = ┬
            '\u{2500}' => '\u{2534}',                     // ─ + up    = ┴
            '\u{2502}' if dirs & RIGHT != 0 => '\u{251C}', // │ + right = ├
            '\u{2502}' => '\u{2524}',                     // │ + left  = ┤
            _ => match dirs {
                d if d == UP | LEFT => '\u{256F}',    // ╯
                d if d == UP | RIGHT => '\u{2570}',   // ╰
                d if d == DOWN | LEFT => '\u{256E}',  // ╮
                d if d == DOWN | RIGHT => '\u{256D}', // ╭
                d if d & (UP | DOWN) != 0 => '\u{2502}',
                _ => '\u{2500}',
            },
        };
        self.set(row, col, ch, Style::default().fg(ink));
    }

    /// The grid as lines, each run of same-styled cells one span, trailing
    /// blanks dropped so a short row costs nothing.
    fn lines(&self) -> Vec<Line<'static>> {
        (0..MAP_H)
            .map(|row| {
                let cells = &self.cells[row * MAP_W..(row + 1) * MAP_W];
                let end = cells
                    .iter()
                    .rposition(|(c, _)| *c != ' ')
                    .map_or(0, |i| i + 1);
                let mut spans: Vec<Span<'static>> = Vec::new();
                for &(ch, style) in &cells[..end] {
                    match spans.last_mut() {
                        Some(last) if last.style == style => last.content.to_mut().push(ch),
                        _ => spans.push(Span::styled(ch.to_string(), style)),
                    }
                }
                Line::from(spans)
            })
            .collect()
    }
}

type Progress<'a> = std::collections::HashMap<&'a str, &'a super::world::RegionProgress>;

/// The whole picture. Its width is fixed by the layout, and `lands_fit`
/// refuses to draw it into anything narrower, so `height` is the only thing
/// the terminal decides here: whether the scroll key is worth mentioning.
/// Split from the render so the layout can be read in a test rather than only
/// on a screen.
fn land_map_lines(atlas: &[super::world::RegionProgress], height: usize) -> Vec<Line<'static>> {
    let progress: Progress = atlas.iter().map(|r| (r.name, r)).collect();
    let mut canvas = Canvas::new();
    for keep in KEEPS {
        canvas.keep(keep, &progress);
    }
    for place in PLACES {
        canvas.label(place.region, place.row, place.at, &progress);
    }
    for road in ROADS {
        canvas.road(road, &progress);
    }

    let total = atlas.len();
    let walked = atlas.iter().filter(|r| r.explored > 0).count();
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                "THE LANDS OF LATEANIA",
                Style::default()
                    .fg(theme::AMBER_GLOW())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("   {walked} of {total} walked"),
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ]),
        Line::from(Span::styled(
            "Every line is a road you can walk. Numbers are zones you have entered.",
            Style::default().fg(theme::TEXT_FAINT()),
        )),
        Line::raw(""),
    ];
    lines.extend(canvas.lines());
    lines.push(Line::raw(""));

    let dim = Style::default().fg(theme::TEXT_DIM());
    let mut ways = vec![Span::styled("Only the Ways reach:  ", dim)];
    for (i, region) in super::worldmap::portal_lands().iter().enumerate() {
        if i > 0 {
            ways.push(Span::styled(
                " \u{00B7} ",
                Style::default().fg(theme::BORDER()),
            ));
        }
        ways.push(Span::styled(
            land_chip_name(region),
            land_style(progress.get(region).copied()),
        ));
    }
    lines.push(Line::from(ways));
    // The Ways line is the only place the portal-only lands appear at all, so
    // it gets air around it rather than reading as the legend's first row.
    lines.push(Line::raw(""));
    // Colour is the only thing separating a land you have seen from one you
    // have not, so say what it means rather than leaving it to be guessed.
    lines.push(Line::from(vec![
        Span::styled("walked", Style::default().fg(theme::TEXT_BRIGHT())),
        Span::styled("  \u{00B7}  ", Style::default().fg(theme::BORDER())),
        Span::styled("not yet", Style::default().fg(theme::TEXT_FAINT())),
        Span::styled("  \u{00B7}  ", Style::default().fg(theme::BORDER())),
        Span::styled(
            "where you stand",
            Style::default()
                .fg(theme::MENTION())
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::raw(""));
    // Only offer the scroll key when there is something below the fold.
    let label = match lines.len() + 1 > height {
        true => "back to the room  [ ] scroll",
        false => "back to the room",
    };
    lines.push(hint("m", label));
    lines
}

/// A land name as the map labels it: the atlas title without its trailing
/// clause and without a leading "The", so "Kaelmyr, the Ashen Reach" reads as
/// "Kaelmyr" and the picture fits a terminal.
fn land_chip_name(region: &str) -> String {
    land_label(region)
        .strip_prefix("The ")
        .unwrap_or(land_label(region))
        .to_string()
}

fn land_style(progress: Option<&super::world::RegionProgress>) -> Style {
    match progress {
        Some(p) if p.here => Style::default()
            .fg(theme::MENTION())
            .add_modifier(Modifier::BOLD),
        Some(p) if p.explored > 0 => Style::default().fg(theme::TEXT_BRIGHT()),
        _ => Style::default().fg(theme::TEXT_FAINT()),
    }
}

// ---- The overhead map page: viewport, legend, and compass (5.1) ----------

/// Paint the room where the tracked walk leaves this floor or land, once it
/// is in view. A stair on the way swaps its double arrow for the walk's own
/// single arrow in the tracked colour: every stair is already the map's
/// green, so a recolour alone said nothing, and the single arrow is the same
/// one the border draws for the walk. A flat crossing keeps its room glyph
/// and only takes the colour. `cells` are the map body as `map_canvas` lays
/// it out: rooms on even offsets from the centre, a stair in the corner cell
/// up and to the right of its room.
#[allow(clippy::too_many_arguments)]
fn paint_track_aim(
    cells: &mut [Vec<(String, Style)>],
    coords: &std::collections::HashMap<super::world::RoomId, super::worldmap::Coord>,
    center: super::worldmap::Coord,
    cols: i32,
    height: i32,
    track: Option<super::worldmap::TrackAim>,
    player_room: super::world::RoomId,
    style: Style,
) {
    let aim_cell = |room: super::world::RoomId, dcol: i32, drow: i32| {
        let c = coords.get(&room)?;
        let sc = cols / 2 + 2 * (c.x - center.x) + dcol;
        let sr = height / 2 + 2 * (c.y - center.y) + drow;
        ((0..cols).contains(&sc) && (0..height).contains(&sr)).then_some((sr as usize, sc as usize))
    };
    match track {
        Some(super::worldmap::TrackAim::Stair { room, climb }) => {
            let glyph = match climb {
                super::worldmap::Climb::Down => '\u{2193}', // ↓
                super::worldmap::Climb::Up => '\u{2191}',   // ↑
            };
            if let Some((row, col)) = aim_cell(room, 1, -1) {
                cells[row][col] = (glyph.to_string(), style);
            }
        }
        Some(super::worldmap::TrackAim::Crossing { room }) if room != player_room => {
            if let Some((row, col)) = aim_cell(room, 0, 0) {
                cells[row][col].1 = style;
            }
        }
        Some(super::worldmap::TrackAim::Crossing { .. })
        | Some(super::worldmap::TrackAim::Target)
        | None => {}
    }
}

fn draw_world_map(frame: &mut Frame, area: Rect, state: &State, view: &PlayerView) {
    use super::world::region_atlas_entry;
    use super::worldmap::{Tile, map_canvas, poi, poi_arrows, world_coords};

    let coords = world_coords();
    let Some(player_room) = view.room else {
        return;
    };
    let Some(&player) = coords.get(&player_room) else {
        frame.render_widget(
            Paragraph::new("No map for this place.").style(Style::default().fg(theme::TEXT_DIM())),
            area,
        );
        return;
    };
    let camera = state.map_camera();
    let level_offset = camera.level_offset();
    let panned = camera.scroll() != (0, 0) || level_offset != 0;
    let center = camera.center(player);

    // Header and map body are never negotiable; everything under them is shed
    // in worst-first order by `MapChrome`, so a phone gets a map instead of a
    // stack of clipped legends with four rows of world under it.
    let chrome = MapChrome::for_area(area);
    let mut constraints = vec![
        Constraint::Length(1), // header
        Constraint::Min(1),    // map body
    ];
    constraints.extend(chrome.footer_constraints());
    let rows = Layout::vertical(constraints).split(area);
    // Footer rects in the order `MapChrome` laid them out; each block below
    // takes the next one only if its row survived the shed.
    let mut footer = rows[2..].iter().copied();
    let inspector_row = (chrome.inspector > 0).then(|| footer.next()).flatten();
    let controls_row = chrome.controls.then(|| footer.next()).flatten();
    let symbols_row = chrome.symbols.then(|| footer.next()).flatten();
    let markers_row = chrome.markers.then(|| footer.next()).flatten();
    let terrain_row = chrome.terrain.then(|| footer.next()).flatten();

    // Header: region name, where this zone sits in the region's chain, the
    // zone's own name, the danger tier, and the current level (z).
    let (region_name, tier) = region_atlas_entry(player_room).unwrap_or(("The wilds", ""));
    let level = level_label(player_room, center.z, player.z);
    let mut header = vec![Span::styled(
        region_name.to_string(),
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD),
    )];
    // A continent is ~20 zones chained one below the next, and each is its own
    // reserved block in the coordinate field, so crossing between them replaces
    // everything on screen. The picture alone therefore can never say that you
    // are 7 zones into a run of 20; naming that is what turns "lost somewhere
    // in a forest" back into a position. Only the procedurally-chained regions
    // can answer it, and elsewhere the header simply stays as it was.
    if let Some(place) = super::world::region_layout(player_room)
        && place.zone_count > 1
    {
        header.push(Span::styled(
            format!("  ·  zone {} of {}", place.zone + 1, place.zone_count),
            Style::default().fg(theme::TEXT_BRIGHT()),
        ));
    }
    if !view.zone.is_empty() && view.zone != region_name {
        header.push(Span::styled(
            format!("  ·  {}", view.zone),
            Style::default().fg(theme::TEXT_BRIGHT()),
        ));
    }
    if !tier.is_empty() {
        header.push(Span::styled(
            format!("  ·  {tier}"),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    let level_style = if level_offset == 0 {
        Style::default().fg(theme::TEXT_FAINT())
    } else {
        // Viewing a different floor than the one you stand on.
        Style::default().fg(theme::AMBER())
    };
    header.push(Span::styled(format!("  ·  {level}"), level_style));
    if panned {
        // The header names where the player stands; the inspector below names
        // the crosshair. Say so, or a panned map reads as two contradictions.
        header.push(Span::styled(
            "  ·  panned (Enter re-centres)",
            Style::default().fg(theme::AMBER()),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(header)), rows[0]);

    // Body: the interleaved canvas - rooms with the corridors between linked
    // rooms drawn in, so paths are visible. Fog hides unvisited rooms and any
    // corridor into the unknown. Boss/taming rooms show a marker; off-screen
    // bosses/POIs get a border arrow pointing the way (no location spoiler).
    let body = rows[1];
    let cols = body.width as i32;
    let height = body.height as i32;
    let cx = (cols / 2) as usize;
    let cy = (height / 2) as usize;
    // The room the player marked, resolved once per frame rather than per cell.
    let dest_room = state.dest_room();
    // Active-quest targets, when the overlay is on (`q`). Same-block targets
    // get a `!` on their cell or a border arrow; cross-block ones are only
    // counted - across reserved blocks an arrow's direction means nothing.
    let quest_targets: Vec<super::world::RoomId> = if state.map_quests() {
        view.quests
            .iter()
            .filter(|q| !q.done)
            .filter_map(|q| q.target)
            .collect()
    } else {
        Vec::new()
    };
    let quest_cells: std::collections::HashSet<super::world::RoomId> =
        quest_targets.iter().copied().collect();
    let canvas = map_canvas(coords, center, cols, height, &view.visited, player_room);

    let player_style = Style::default()
        .fg(Color::Rgb(250, 240, 140))
        .add_modifier(Modifier::BOLD);
    let boss_style = Style::default()
        .fg(Color::Rgb(250, 210, 90))
        .add_modifier(Modifier::BOLD);
    let tame_style = Style::default()
        .fg(Color::Rgb(230, 140, 160))
        .add_modifier(Modifier::BOLD);
    let gather_style = Style::default()
        .fg(Color::Rgb(150, 200, 120))
        .add_modifier(Modifier::BOLD);
    let elite_style = Style::default()
        .fg(Color::Rgb(210, 120, 90))
        .add_modifier(Modifier::BOLD);
    let link_style = Style::default().fg(theme::BORDER_DIM());
    let quest_style = Style::default()
        .fg(theme::SUCCESS())
        .add_modifier(Modifier::BOLD);
    let mut cells: Vec<Vec<(String, Style)>> = canvas
        .iter()
        .map(|row| {
            row.iter()
                .map(|tile| match tile {
                    Tile::Empty => (" ".to_string(), Style::default()),
                    Tile::LinkH => ("\u{2500}".to_string(), link_style), // ─
                    Tile::LinkV => ("\u{2502}".to_string(), link_style), // │
                    Tile::Hint(ch) => (ch.to_string(), Style::default().fg(theme::TEXT_FAINT())),
                    // A known non-Euclidean jump (already explored, just not
                    // adjacent on screen): brighter than a plain fog hint, so
                    // it reads as "goes somewhere you've been", not the edge
                    // of the map.
                    Tile::HintKnown(ch) => (
                        ch.to_string(),
                        Style::default()
                            .fg(theme::AMBER_DIM())
                            .add_modifier(Modifier::BOLD),
                    ),
                    // The ways up and down out of the room beside it. A flat
                    // level has no direction to draw these in, and in a world
                    // chained zone-to-zone by stairs they are usually the way
                    // onward, so they get their own corner cell.
                    Tile::Stair(ch) => (ch.to_string(), stair_style()),
                    Tile::Room(id) if *id == player_room => ("@".to_string(), player_style),
                    // Where you said you were going, outranking every other
                    // marker: once a destination is marked, a boss star on
                    // that room is not what you opened the map to find.
                    Tile::Room(id) if Some(*id) == dest_room => (
                        "\u{2691}".to_string(),
                        Style::default()
                            .fg(theme::SUCCESS())
                            .add_modifier(Modifier::BOLD),
                    ),
                    // An active quest's target room, when the overlay is on.
                    // Below the marked destination (a deliberate mark beats a
                    // standing hint) but above the boss star.
                    Tile::Room(id) if quest_cells.contains(id) => ("!".to_string(), quest_style),
                    Tile::Room(id) => match poi(*id) {
                        Some(p) if p.boss.is_some() => ("\u{2605}".to_string(), boss_style),
                        Some(p) if p.tameable.is_some() => ("\u{2665}".to_string(), tame_style),
                        Some(p) if p.elite_foe.is_some() => ("\u{25c6}".to_string(), elite_style),
                        Some(p) if p.gather.is_some() => ("\u{2692}".to_string(), gather_style),
                        _ => {
                            let (g, color) = biome_style(super::world::biome_of(*id));
                            (g.to_string(), Style::default().fg(color))
                        }
                    },
                })
                .collect()
        })
        .collect();

    // Off-screen POI direction arrows, hugging the explored cluster's edge.
    for arrow in hug_poi_arrows(poi_arrows(coords, center, cols, height), &canvas) {
        if let Some(cell) = cells.get_mut(arrow.row).and_then(|r| r.get_mut(arrow.col)) {
            let style = if arrow.boss { boss_style } else { tame_style };
            *cell = (arrow.glyph.to_string(), style);
        }
    }
    // The green arrow is the one you chose, and it works exactly like the
    // amber ones: a straight-line direction, drawn only within `PAN_LIMIT`
    // (same land, where the coordinate delta is a real spatial relationship).
    // Crucially this needs no `visited` at all, so it points at a boss you
    // have never found - which is the whole job of tracking a quest. Drawn
    // after (over) the amber arrows: a border cell can only say one thing,
    // and where-you're-going beats where-a-boss-is.
    //
    // On your own floor it aims along the real walk (`worldmap::track_aim`):
    // at the destination while the walk stays on this floor and in this land,
    // else at the room where the walk leaves them, whose stair swaps to the
    // walk's single arrow (or the room itself, for a flat crossing into
    // another land, turns green) once in view. So any destination can be
    // tracked, however far. Viewing another
    // floor (`<`/`>`) aims straight at the destination if it is on that floor.
    if let Some(dest) = dest_room {
        let track = if level_offset == 0 {
            state.dest_track_aim()
        } else {
            Some(super::worldmap::TrackAim::Target)
        };
        let aim = match track {
            None => None,
            Some(super::worldmap::TrackAim::Target) => Some(dest),
            Some(super::worldmap::TrackAim::Stair { room, .. })
            | Some(super::worldmap::TrackAim::Crossing { room }) => Some(room),
        };
        if let Some(aim) = aim {
            let (dest_arrows, _) =
                super::worldmap::quest_arrows(coords, center, cols, height, &[aim]);
            for arrow in hug_poi_arrows(dest_arrows, &canvas) {
                if let Some(cell) = cells.get_mut(arrow.row).and_then(|r| r.get_mut(arrow.col)) {
                    *cell = (arrow.glyph.to_string(), quest_style);
                }
            }
        }
        paint_track_aim(
            &mut cells,
            coords,
            center,
            cols,
            height,
            track,
            player_room,
            quest_style,
        );
    }
    // Cross-land quest targets are counted in the footer instead of pointed
    // at with a meaningless direction.
    let quests_beyond = if quest_targets.is_empty() {
        0
    } else {
        super::worldmap::quest_arrows(coords, center, cols, height, &quest_targets).1
    };

    // Land labels: name each explored region once, near the centroid of its
    // rooms in view, so you can see which land is which at a glance. Only lands
    // you've set foot in are drawn (canvas already holds only seen rooms), and
    // the text lands only in empty cells so it never hides a room or a marker.
    let mut land_centroids: std::collections::HashMap<&'static str, (i32, i32, i32)> =
        std::collections::HashMap::new();
    for (r, row) in canvas.iter().enumerate() {
        for (c, tile) in row.iter().enumerate() {
            if let Tile::Room(id) = tile
                && let Some((name, _)) = region_atlas_entry(*id)
            {
                let e = land_centroids.entry(name).or_insert((0, 0, 0));
                e.0 += r as i32;
                e.1 += c as i32;
                e.2 += 1;
            }
        }
    }
    let label_style = Style::default()
        .fg(theme::AMBER_DIM())
        .add_modifier(Modifier::ITALIC);
    let rows_n = cells.len();
    for (name, (sum_r, sum_c, count)) in land_centroids {
        // A stray room or two shouldn't plant a label; wait for a real presence.
        if count < 3 {
            continue;
        }
        let text: Vec<char> = land_label(name).chars().collect();
        let len = text.len();
        let mid_r = sum_r / count;
        let ideal_c = ((sum_c / count) - (len as i32) / 2).max(0);

        // Find a clear horizontal run of `len` cells near the centroid so the
        // name reads whole rather than being chopped up by rooms and corridors.
        // Scan rows outward from the centroid, nudging left/right a little; if
        // nothing clear turns up, drop the label - better absent than garbled.
        let is_clear = |cells: &[Vec<(String, Style)>], r: usize, c0: usize| {
            c0 + len <= cols as usize && (0..len).all(|i| cells[r][c0 + i].0 == " ")
        };
        let mut spot = None;
        'search: for dr in 0..=5i32 {
            for r in [mid_r - dr, mid_r + dr] {
                if r < 0 || r as usize >= rows_n {
                    continue;
                }
                let r = r as usize;
                for dc in 0..=10i32 {
                    for c in [ideal_c + dc, ideal_c - dc] {
                        if c >= 0 && is_clear(&cells, r, c as usize) {
                            spot = Some((r, c as usize));
                            break 'search;
                        }
                    }
                }
                if dr == 0 {
                    break; // mid_r - 0 == mid_r + 0; don't scan it twice
                }
            }
        }
        if let Some((r, c0)) = spot {
            for (i, ch) in text.iter().enumerate() {
                cells[r][c0 + i] = (ch.to_string(), label_style);
            }
        }
    }

    // Crosshair at the viewport centre (the inspector target).
    if let Some(cell) = cells.get_mut(cy).and_then(|r| r.get_mut(cx)) {
        cell.1 = cell.1.add_modifier(Modifier::REVERSED);
    }

    let lines: Vec<Line> = cells
        .into_iter()
        .map(|row| {
            Line::from(
                row.into_iter()
                    .map(|(ch, style)| Span::styled(ch, style))
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), body);

    // Inspector: the room under the crosshair (canvas centre). Fog already
    // blanked unvisited rooms, so a room here is one the player has seen.
    let cursor_room = match canvas.get(cy).and_then(|r| r.get(cx)) {
        Some(Tile::Room(id)) => Some(*id),
        _ => None,
    };
    let mut inspect: Vec<Line> = Vec::new();
    if let Some(id) = cursor_room {
        let (rn, rt) = region_atlas_entry(id).unwrap_or(("The wilds", ""));
        let mut top = vec![
            Span::styled("\u{25ce} ", Style::default().fg(theme::AMBER())), // ◎
            Span::styled(rn.to_string(), Style::default().fg(theme::TEXT_BRIGHT())),
        ];
        if !rt.is_empty() {
            top.push(Span::styled(
                format!("  ·  {rt}"),
                Style::default().fg(theme::TEXT_DIM()),
            ));
        }
        inspect.push(Line::from(top));

        let mut parts: Vec<String> = Vec::new();
        if let Some(p) = super::worldmap::poi(id) {
            if let Some(boss) = p.boss {
                if p.reward.is_empty() {
                    parts.push(format!("boss {boss}"));
                } else {
                    parts.push(format!("boss {boss} (drops {})", p.reward.join(", ")));
                }
            }
            if let Some(t) = p.tameable {
                parts.push(format!("tame {t}"));
            }
            if let Some(g) = p.gather {
                parts.push(format!("gather {} (needs Lv{})", g.skill, g.level_req));
            }
            if !p.monsters.is_empty() {
                parts.push(format!("foes {}", p.monsters.join(", ")));
            }
        }
        let poi_text = if parts.is_empty() {
            "no notable sites here".to_string()
        } else {
            parts.join("  ·  ")
        };
        inspect.push(Line::from(Span::styled(
            poi_text,
            Style::default().fg(theme::TEXT_DIM()),
        )));
    } else {
        inspect.push(Line::from(Span::styled(
            "unexplored",
            Style::default().fg(theme::TEXT_FAINT()),
        )));
    }
    // The map replaces the room panel while it is open, so the heading it just
    // set would otherwise be invisible until the map is closed again. Confirm
    // the mark here instead, on the inspector's own second row.
    if let Some(heading) = state.heading() {
        let (text, color) = match heading {
            Heading::Toward(name, route) => (
                format!(
                    "{} compass: {name} · {} room{} · take {}",
                    route.next.compass_glyph(),
                    route.rooms,
                    if route.rooms == 1 { "" } else { "s" },
                    route.next.label()
                ),
                theme::SUCCESS(),
            ),
            Heading::Arrived(name) => (format!("\u{2691} {name} · you're here"), theme::SUCCESS()),
            Heading::Unreachable(name) => (
                format!("\u{2715} {name} · no way there over ground you know"),
                theme::ERROR(),
            ),
        };
        inspect.truncate(1);
        inspect.push(Line::from(Span::styled(text, Style::default().fg(color))));
    }
    if let Some(rect) = inspector_row {
        // On a one-row inspector something has to give. A heading outranks the
        // region name - it is the line that says which way to walk - and with
        // no heading the region name outranks the list of what is in the room,
        // because "where is the crosshair" is the question the crosshair asks.
        if chrome.inspector == 1 && inspect.len() > 1 {
            match state.heading().is_some() {
                true => drop(inspect.drain(..inspect.len() - 1)),
                false => inspect.truncate(1),
            }
        }
        frame.render_widget(Paragraph::new(inspect), rect);
    }

    let dim = Style::default().fg(theme::TEXT_DIM());
    // Footer line 1: controls.
    if let Some(rect) = controls_row {
        frame.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                map_controls_line(area.width),
                dim,
            )])),
            rect,
        );
    }

    // Footer line 2: what the map's own symbols mean - the glyphs every map
    // shows regardless of what's actually nearby (rooms, corridors, the two
    // kinds of "more lies this way" stub, the look-here cursor). Stubs, never
    // arrows, on purpose: arrows read as controls here, a line just means
    // "walkable path".
    if let Some(rect) = symbols_row {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("@", player_style),
                Span::styled(" you  ", dim),
                Span::styled("\u{2500}\u{2502}", link_style),
                Span::styled(" known path  ", dim),
                Span::styled("\u{2500}\u{2502}", Style::default().fg(theme::TEXT_FAINT())),
                Span::styled(" unexplored  ", dim),
                Span::styled(
                    "\u{2500}\u{2502}",
                    Style::default()
                        .fg(theme::AMBER_DIM())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" known, elsewhere  ", dim),
                Span::styled("\u{21d3}\u{21d1}", stair_style()),
                Span::styled(" way down/up  ", dim),
                Span::styled(" ", Style::default().add_modifier(Modifier::REVERSED)),
                Span::styled(" look here", dim),
            ])),
            rect,
        );
    }

    // Footer line 3: marker legend (quests, bosses, tames, notable foes,
    // gather nodes, and the border arrow for an off-screen one of those).
    if let Some(rect) = markers_row {
        let mut marker_legend = vec![
            Span::styled("!", quest_style),
            Span::styled(" quest  ", dim),
            Span::styled("\u{2605}", Style::default().fg(Color::Rgb(250, 210, 90))),
            Span::styled(" boss  ", dim),
            Span::styled("\u{2665}", Style::default().fg(Color::Rgb(230, 140, 160))),
            Span::styled(" tame  ", dim),
            Span::styled("\u{25c6}", Style::default().fg(Color::Rgb(210, 120, 90))),
            Span::styled(" notable foe  ", dim),
            Span::styled("\u{2692}", Style::default().fg(Color::Rgb(150, 200, 120))),
            Span::styled(" gather  ", dim),
            Span::styled("\u{2192}", Style::default().fg(theme::AMBER_DIM())),
            Span::styled(" one of these, off-map  ", dim),
            Span::styled("\u{2691}\u{2192}", quest_style),
            Span::styled(" tracked", dim),
        ];
        if quests_beyond > 0 {
            // An honest count instead of a dishonest arrow: these targets sit in
            // other lands, where a border direction would mean nothing.
            marker_legend.push(Span::styled(
                format!(
                    "  \u{00b7} {quests_beyond} quest{} beyond this land (track: j)",
                    if quests_beyond == 1 { "" } else { "s" }
                ),
                Style::default().fg(theme::SUCCESS()),
            ));
        }
        frame.render_widget(Paragraph::new(Line::from(marker_legend)), rect);
    }

    // Footer line 4: terrain key, showing only the biomes actually in view so it
    // stays legible instead of listing every biome in the world. Walking the
    // whole canvas for it is only worth doing when there is a row to print it
    // on.
    let Some(terrain_rect) = terrain_row else {
        return;
    };
    use super::world::Biome;
    let mut present: Vec<Biome> = Vec::new();
    for row in &canvas {
        for tile in row {
            if let Tile::Room(id) = tile {
                let b = super::world::biome_of(*id);
                if !present.contains(&b) {
                    present.push(b);
                }
            }
        }
    }
    // Fixed order so the key doesn't reshuffle as you pan.
    const BIOME_ORDER: [(Biome, &str); 9] = [
        (Biome::Heartland, "heartland"),
        (Biome::Plains, "plains"),
        (Biome::Urban, "town"),
        (Biome::Forest, "forest"),
        (Biome::Water, "water"),
        (Biome::Islands, "isles"),
        (Biome::Ash, "ash"),
        (Biome::Cavern, "caves"),
        (Biome::Badlands, "badlands"),
    ];
    let mut key: Vec<Span> = vec![Span::styled(
        "terrain  ",
        Style::default().fg(theme::TEXT_FAINT()),
    )];
    for (biome, label) in BIOME_ORDER {
        if !present.contains(&biome) {
            continue;
        }
        let (glyph, color) = biome_style(biome);
        key.push(Span::styled(glyph.to_string(), Style::default().fg(color)));
        key.push(Span::styled(format!(" {label}  "), dim));
    }
    frame.render_widget(Paragraph::new(Line::from(key)), terrain_rect);
}

// ---- The one-time gates: class select and archetype select ---------------

fn draw_class_select(frame: &mut Frame, area: Rect, view: &PlayerView, cursor: usize) {
    let cursor = cursor.min(Class::ALL.len() - 1);
    let chosen = Class::ALL[cursor];
    let accent = class_accent(Some(chosen));
    let mut lines = vec![
        Line::from(Span::styled(
            "~ LATEANIA ~",
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "Choose your calling. w/s to move, Enter to choose (or press 1-9).",
            Style::default().fg(theme::TEXT_DIM()),
        )),
        Line::raw(""),
        Line::from(vec![
            Span::styled(
                "Your rolled fate (4d6, drop lowest): ",
                Style::default().fg(theme::AMBER()),
            ),
            Span::styled(
                "press r to reroll until you choose",
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ]),
    ];
    // The rolled scores in the same rated rows as the character sheet, with the
    // highlighted class's primary score glowing in its accent.
    lines.extend(attribute_lines(view, primary_label(Some(chosen)), accent));
    lines.extend(attribute_rule_lines(view));
    lines.push(Line::raw(""));
    // One compact row per class; the highlighted one is expanded directly below
    // its row so cursor-following scroll keeps the choice and its details together.
    let mut selected_detail_line = 0;
    for (i, class) in Class::ALL.iter().enumerate() {
        let selected = i == cursor;
        let marker = if selected { ">" } else { " " };
        let quick = if i < 9 {
            format!("{}", i + 1)
        } else {
            "·".to_string()
        };
        let name_style = if selected {
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .patch(theme::selection_style())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD)
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("{marker} {quick} "),
                Style::default().fg(theme::AMBER()),
            ),
            Span::styled(format!("{:<12}", class.name()), name_style),
            Span::styled(
                format!("  {}", class.tagline()),
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ]));
        if selected {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {} ", chosen.name()),
                    Style::default()
                        .fg(theme::AMBER_GLOW())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(
                        "· {} · trait: {}",
                        chosen.resource().label(),
                        chosen.trait_name()
                    ),
                    Style::default().fg(theme::AMBER_DIM()),
                ),
            ]));
            lines.push(Line::from(Span::styled(
                format!("    {}", chosen.trait_desc()),
                Style::default().fg(theme::TEXT()),
            )));
            selected_detail_line = lines.len() - 1;
        }
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "World by Tasmania - thanks to late.sh and its contributors.",
        Style::default().fg(theme::TEXT_FAINT()),
    )));
    let off = scroll_offset(
        0,
        &lines,
        Some(selected_detail_line),
        area.width as usize,
        area.height as usize,
    );
    let shown: Vec<Line<'static>> = lines.into_iter().skip(off).collect();
    frame.render_widget(Paragraph::new(shown).wrap(Wrap { trim: false }), area);
}

/// What the six scores do, in numbers, for this character: each score's
/// current reading and the rule behind it. Under the rolled rows on the
/// creation screen, so the roll is a decision made with the arithmetic in
/// view rather than a promise.
fn attribute_rule_lines(view: &PlayerView) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(Span::styled(
        format!(
            "What they do (modifier = (score-10)/2; a point to place every {POINT_EVERY_LEVELS} levels, scores cap at {SCORE_CAP}):"
        ),
        Style::default().fg(theme::TEXT_DIM()),
    ))];
    // The effect column is as wide as the longest reading on screen plus a
    // fixed gap, so no row ever runs into its rule.
    const GAP: usize = 2;
    let effects: Vec<String> = Score::ALL
        .iter()
        .map(|which| view.scores.effect(*which, view.level))
        .collect();
    let width = effects.iter().map(|e| e.chars().count()).max().unwrap_or(0) + GAP;
    for (which, effect) in Score::ALL.iter().zip(effects) {
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {} ", which.label()),
                Style::default().fg(theme::AMBER()),
            ),
            Span::styled(
                format!("{effect:<width$}"),
                Style::default().fg(theme::TEXT()),
            ),
            Span::styled(
                which.rule().to_string(),
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ]));
    }
    lines
}

/// The attribute point screen: every score with what it does now, what one
/// more point would do, and the rule behind it, placed with 1-6. A +1 on an
/// even score leaves the modifier where it is, and the row says so rather
/// than let the player wonder why nothing moved. The screen holds every key
/// until the point is placed, so all six choices must be in view: the full
/// layout is five rows a score, and when `rows` (the area's height) cannot
/// hold it every score collapses to one line.
fn score_point_lines(view: &PlayerView, rows: u16) -> Vec<Line<'static>> {
    const HEADER_ROWS: usize = 4;
    const ROWS_PER_SCORE: usize = 5;
    let full = HEADER_ROWS + ROWS_PER_SCORE * view.score_offer.len();
    if full > usize::from(rows) {
        return score_point_lines_compact(view);
    }
    let mut lines = vec![
        Line::from(Span::styled(
            "~ YOU GROW ~",
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!(
                "Level {} - {} attribute point(s) to place. Modifier = (score-10)/2; scores cap at {SCORE_CAP}.",
                view.level, view.score_points
            ),
            Style::default().fg(theme::TEXT_DIM()),
        )),
        Line::from(Span::styled(
            "Press 1-6 to place a point.",
            Style::default().fg(theme::TEXT_DIM()),
        )),
        Line::raw(""),
    ];
    for (i, row) in view.score_offer.iter().enumerate() {
        let sign = if row.modifier >= 0 { "+" } else { "" };
        lines.push(Line::from(vec![
            Span::styled(format!("  {} ", i + 1), Style::default().fg(theme::AMBER())),
            Span::styled(
                format!("{} {} ({sign}{}) ", row.label, row.value, row.modifier),
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("· {}", row.name),
                Style::default().fg(theme::AMBER_DIM()),
            ),
        ]));
        lines.push(Line::from(Span::styled(
            format!("      now: {}", row.now),
            Style::default().fg(theme::TEXT()),
        )));
        let after = match &row.after {
            Some(after) if after == &row.now => format!(
                "      +1 -> {}: {after} (the modifier moves at {})",
                row.value + 1,
                row.value + 2
            ),
            Some(after) => format!("      +1 -> {}: {after}", row.value + 1),
            None => format!("      at the cap of {SCORE_CAP}"),
        };
        lines.push(Line::from(Span::styled(
            after,
            Style::default().fg(theme::SUCCESS()),
        )));
        lines.push(Line::from(Span::styled(
            format!("      {}", row.rule),
            Style::default().fg(theme::TEXT_DIM()),
        )));
        lines.push(Line::raw(""));
    }
    lines
}

/// The point screen for a short terminal: one line a score, now -> after.
fn score_point_lines_compact(view: &PlayerView) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                "~ YOU GROW ~ ",
                Style::default()
                    .fg(theme::AMBER_GLOW())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    "Level {} - {} attribute point(s) to place. Press 1-6 to place a point (scores cap at {SCORE_CAP}).",
                    view.level, view.score_points
                ),
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ]),
        Line::raw(""),
    ];
    for (i, row) in view.score_offer.iter().enumerate() {
        let sign = if row.modifier >= 0 { "+" } else { "" };
        let after = match &row.after {
            Some(after) if after == &row.now => format!("-> the same until {}", row.value + 2),
            Some(after) => format!("-> {after}"),
            None => format!("at the cap of {SCORE_CAP}"),
        };
        let mut spans = vec![
            Span::styled(format!("  {} ", i + 1), Style::default().fg(theme::AMBER())),
            Span::styled(
                format!("{} {} ({sign}{}) ", row.label, row.value, row.modifier),
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
        ];
        if row.after.is_some() {
            spans.push(Span::styled(
                format!("{} ", row.now),
                Style::default().fg(theme::TEXT()),
            ));
        }
        spans.push(Span::styled(after, Style::default().fg(theme::SUCCESS())));
        spans.push(Span::styled(
            format!(" · {}", row.hint),
            Style::default().fg(theme::TEXT_DIM()),
        ));
        lines.push(Line::from(spans));
    }
    lines
}

/// The level-10 archetype crossroads: two permanent paths, picked with 1/2.
fn draw_archetype_select(frame: &mut Frame, area: Rect, view: &PlayerView) {
    let mut lines = vec![
        Line::from(Span::styled(
            "~ A PATH OPENS ~",
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!(
                "Level {} - your {} must choose a calling. This is permanent.",
                view.level, view.class_name
            ),
            Style::default().fg(theme::TEXT_DIM()),
        )),
        Line::from(Span::styled(
            "Press 1 or 2 to commit to a path.",
            Style::default().fg(theme::TEXT_DIM()),
        )),
        Line::raw(""),
    ];
    for (i, (name, role, desc)) in view.archetype_choices.iter().enumerate() {
        lines.push(Line::from(vec![
            Span::styled(format!("  {} ", i + 1), Style::default().fg(theme::AMBER())),
            Span::styled(
                format!("{name} "),
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("· {role}"), Style::default().fg(theme::AMBER_DIM())),
        ]));
        lines.push(Line::from(Span::styled(
            format!("      {desc}"),
            Style::default().fg(theme::TEXT()),
        )));
        lines.push(Line::raw(""));
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

// ---- The side column: log, compact mode, and list scrolling --------------

fn side_paragraph(lines: Vec<Line<'static>>) -> Paragraph<'static> {
    Paragraph::new(lines).wrap(Wrap { trim: false })
}

fn draw_compact(frame: &mut Frame, area: Rect, view: &PlayerView) {
    let mut lines = vec![Line::from(vec![
        Span::styled(
            view.room_name.clone(),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  {}/{}hp", view.hp, view.max_hp),
            Style::default().fg(hp_color(view.hp, view.max_hp)),
        ),
    ])];
    lines.extend(wrapped_log_tail(
        view,
        area.width as usize,
        area.height.saturating_sub(1) as usize,
    ));
    frame.render_widget(side_paragraph(lines), area);
}

/// The field layout's message feed: a full-width strip under the field, newest
/// line at the bottom, a rule above so the world and the words don't bleed into
/// each other. No "Now" block and no header - room context lives in the side
/// panel, and every row here is a line of actual events.
fn draw_log_strip(frame: &mut Frame, area: Rect, view: &PlayerView) {
    if area.height < 2 || area.width == 0 {
        return;
    }
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(area);
    frame.render_widget(
        Paragraph::new(separator_line(rows[0].width as usize)),
        rows[0],
    );
    let width = rows[1].width as usize;
    let entries = collapsed_recent_entries(view);
    let mut lines: Vec<Line<'static>> = entries
        .iter()
        .flat_map(|(kind, text)| wrapped_log_line(*kind, text, width))
        .collect();
    let start = lines.len().saturating_sub(rows[1].height as usize);
    frame.render_widget(Paragraph::new(lines.split_off(start)), rows[1]);
}

fn draw_log(frame: &mut Frame, area: Rect, view: &PlayerView) {
    if area.height < 12 {
        let lines = recent_log_tail(view, area.width as usize, area.height as usize);
        frame.render_widget(Paragraph::new(lines), area);
        return;
    }

    // Mid-fight the room prose gives way to the battle frame: the foe's full
    // name, both sides' meters, and active effects. It reverts the moment the
    // fight ends.
    let context_lines = battle_context(view, area.width as usize)
        .unwrap_or_else(|| current_room_context(view, area.width as usize));
    let recent_reserve = if area.height < 18 { 5 } else { 8 };
    let context_h = (context_lines.len() as u16)
        .min(area.height.saturating_sub(recent_reserve + 1))
        .max(1);
    let rows = Layout::vertical([
        Constraint::Length(context_h),
        Constraint::Length(1),
        Constraint::Min(1),
    ])
    .split(area);
    frame.render_widget(
        Paragraph::new(truncate_lines(context_lines, context_h)),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(separator_line(rows[1].width as usize)),
        rows[1],
    );

    let events = recent_log_tail(view, rows[2].width as usize, rows[2].height as usize);
    frame.render_widget(Paragraph::new(events), rows[2]);
}

fn draw_side(
    frame: &mut Frame,
    area: Rect,
    state: &State,
    view: &PlayerView,
    usernames: &UsernameLookup<'_>,
) {
    if state.panel() == Panel::Room {
        draw_room_side(frame, area, state, view, usernames, true);
        return;
    }

    // List panels return the line index of the highlighted row so the view can
    // scroll to keep the selection visible; text panels return `None`.
    let (lines, selected) = match state.panel() {
        Panel::Room => unreachable!("room panel is rendered by draw_room_side"),
        Panel::Character => (character_panel(view), None),
        Panel::Abilities => abilities_panel(view, state.cursor()),
        Panel::Inventory => inventory_panel(&state.inv_rows(), view, state.cursor()),
        Panel::Shop => shop_panel(&state.shop_rows(), view, state.cursor()),
        Panel::Examine => examine_panel(view, state.cursor()),
        Panel::Titles => titles_panel(view, state.cursor()),
        Panel::Quests => quests_panel(view, state.cursor(), state.dest_room()),
        Panel::Follow => follow_panel(view, state.cursor(), usernames),
        Panel::Stable => stable_panel(view, state.cursor()),
        Panel::Taming => taming_panel(view, state.cursor()),
        Panel::Housing => housing_panel(view, state.cursor()),
        Panel::Portal => portal_panel(view, state.cursor()),
        Panel::Appearance => (appearance_panel(view, state.cursor()), None),
        Panel::Crafting => crafting_panel(&state.craft_rows(), view, state.cursor()),
        Panel::Map => (atlas_panel(view), None),
        Panel::Leaderboard => (leaderboard_panel(view, usernames), None),
        Panel::Board => board_panel(view, state.cursor()),
    };
    let off = scroll_offset(
        state.list_scroll(),
        &lines,
        selected,
        area.width as usize,
        area.height as usize,
    );
    state.set_list_scroll(off);
    let shown = if off == 0 {
        lines
    } else {
        lines.into_iter().skip(off).collect()
    };
    frame.render_widget(side_paragraph(shown), area);
}

/// Lines of context kept between the highlighted row and the top/bottom edges of
/// the list view, so navigating never parks the selection flush against a
/// border (except at the very start/end of the list, where there's nothing more
/// to show).
const LIST_SCROLL_MARGIN: usize = 2;

/// New first-visible *line* for a side panel, given the previous scroll `prev`.
///
/// `side_paragraph` word-wraps (`Wrap { trim: false }`), so one logical line can
/// occupy several terminal rows — the crafting panel's ingredient/gated-reason
/// rows routinely wrap in the 28-34-wide side panel. The scroll therefore counts
/// **wrapped rows**, not logical lines; counting lines used to leave the last
/// several recipes stranded below the screen.
///
/// List panels pass the highlighted line as `selected` and auto-follow it,
/// nudging only when it would come within `LIST_SCROLL_MARGIN` rows of an edge.
/// Cursor-less text panels pass `selected = None` and are scrolled manually
/// (`[` / `]`); the offset is just clamped so it can't overscroll into blank.
fn scroll_offset(
    prev: usize,
    lines: &[Line<'_>],
    selected: Option<usize>,
    width: usize,
    height: usize,
) -> usize {
    let n = lines.len();
    if height == 0 || n == 0 {
        return 0;
    }
    let rows: Vec<usize> = lines.iter().map(|l| line_rows(l, width)).collect();
    let total_rows: usize = rows.iter().sum();
    if total_rows <= height {
        return 0;
    }
    // prefix[i] = wrapped rows above logical line i.
    let mut prefix = vec![0usize; n + 1];
    for i in 0..n {
        prefix[i + 1] = prefix[i] + rows[i];
    }
    // Largest line offset that still fills the screen (never scroll into blank).
    let max_top = total_rows - height;
    let max_off = (0..n).rev().find(|&i| prefix[i] <= max_top).unwrap_or(0);

    let Some(sel) = selected else {
        // Text panel: honor the manual offset, clamped to the content.
        return prev.min(max_off);
    };
    // Margin can't exceed what fits above and below within the window.
    let margin = LIST_SCROLL_MARGIN.min(height.saturating_sub(1) / 2);
    let mut off = prev.min(max_off);
    // Scroll up until the selection's top row clears the margin (or we hit 0).
    while off > 0 && prefix[sel] < prefix[off] + margin {
        off -= 1;
    }
    // Scroll down until the selection's bottom row clears the margin.
    while off < max_off && prefix[sel] + rows[sel] + margin > prefix[off] + height {
        off += 1;
    }
    off.min(max_off)
}

/// Terminal rows a rendered `line` occupies in a `width`-wide side panel,
/// matching `side_paragraph`'s word-wrap so the scroll can count rows.
fn line_rows(line: &Line<'_>, width: usize) -> usize {
    let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    wrapped_rows(&text, width)
}

/// Word-wrap row count for `text` at `width`, approximating ratatui's
/// `WordWrapper` (`Wrap { trim: false }`): break on spaces, split a word longer
/// than the width across rows. Always at least 1.
fn wrapped_rows(text: &str, width: usize) -> usize {
    if width == 0 {
        return 1;
    }
    let mut rows = 1usize;
    let mut col = 0usize;
    for (i, token) in text.split(' ').enumerate() {
        if i > 0 {
            // the single space that `split` consumed between tokens
            if col + 1 > width {
                rows += 1;
                col = 0;
            } else {
                col += 1;
            }
        }
        let tw = UnicodeWidthStr::width(token);
        if tw == 0 {
            continue;
        }
        if col + tw <= width {
            col += tw;
        } else {
            if col > 0 {
                rows += 1;
            }
            if tw > width {
                // a single word longer than the panel breaks across rows
                let extra = (tw - 1) / width;
                rows += extra;
                col = tw - extra * width;
            } else {
                col = tw;
            }
        }
    }
    rows
}

// ---- The room side panel, and the titles panel ---------------------------

fn draw_room_side(
    frame: &mut Frame,
    area: Rect,
    state: &State,
    view: &PlayerView,
    usernames: &UsernameLookup<'_>,
    with_minimap: bool,
) {
    let map = if with_minimap {
        minimap_lines(&view.minimap)
    } else {
        // The live field column already shows the surroundings; the little
        // minimap would just be a redundant echo beside it.
        Vec::new()
    };
    let panel_area = if map.is_empty() {
        area
    } else {
        let map_h = map.len().min(area.height as usize) as u16;
        let rows = Layout::vertical([Constraint::Min(0), Constraint::Length(map_h)]).split(area);
        frame.render_widget(Paragraph::new(map), rows[1]);
        rows[0]
    };

    // Mid-fight the field layout's side panel becomes the battle frame - the
    // classic layout keeps the room summary here, since its main column
    // already swaps to `battle_context`. Its rows carry their own click
    // actions (foes to switch the lock, ability rows to cast).
    let fighting =
        view.mobs.iter().any(|m| m.targeted) || view.occupants.iter().any(|o| o.targeted);
    if !with_minimap && fighting {
        let (lines, hits) = battle_side_panel(view, usernames, panel_area.width as usize);
        for (idx, action) in hits {
            if (idx as u16) < panel_area.height {
                state.record_combat_hit(
                    Rect {
                        x: panel_area.x,
                        y: panel_area.y + idx as u16,
                        width: panel_area.width,
                        height: 1,
                    },
                    action,
                );
            }
        }
        frame.render_widget(Paragraph::new(lines), panel_area);
        return;
    }
    let panel = room_panel(view, usernames, panel_area.width as usize, state.heading());
    let RoomPanel {
        mut body,
        footer,
        foe_hits,
        player_hits,
    } = panel;

    // The standing keys are pinned to the floor of the rail rather than
    // trailing the content. The room's state then reads top-down from the
    // vitals and the keys sit at a fixed edge, instead of the block ending
    // wherever the content happened to stop with dead space below it.
    //
    // Only while it leaves the room itself the larger half: on a cramped rail
    // the footer goes back to being ordinary content at the end of the body,
    // where it scrolls with everything else instead of eating the panel.
    let footer_rows: usize = footer
        .iter()
        .map(|line| line_rows(line, panel_area.width as usize))
        .sum();
    let (body_area, footer_area) = match footer_anchor(footer_rows, panel_area.height) {
        Some(rows) => {
            let split =
                Layout::vertical([Constraint::Min(0), Constraint::Length(rows)]).split(panel_area);
            (split[0], Some(split[1]))
        }
        None => {
            body.push(Line::raw(""));
            body.extend(footer.iter().cloned());
            (panel_area, None)
        }
    };

    // The room panel was the one panel in the game that could not scroll: it
    // rendered every line into a fixed rect and whatever ran past the bottom
    // was silently dropped, so on a short terminal the tail of the panel simply
    // did not exist. It scrolls now, on the same `[`/`]` + `list_scroll` path
    // every other cursor-less panel uses, with the vitals block pinned: your HP
    // must not be something you scroll to find.
    let (lines, pinned, off) = scroll_room_panel(
        body,
        state.list_scroll(),
        body_area.width as usize,
        body_area.height as usize,
    );
    state.set_list_scroll(off);

    // Make each visible foe row clickable: its rect is where the panel (drawn
    // from the top, one pre-wrapped line per row) places that line. A row above
    // the scroll window, or past the bottom, is not on screen and so is not
    // recorded. `screen_row` maps a line index through the pinned head and the
    // scroll offset; getting this wrong aims clicks at the wrong foe.
    let screen_row = |idx: usize| -> Option<u16> {
        let row = if idx < pinned {
            idx
        } else {
            let below = idx - pinned;
            if below < off {
                return None;
            }
            pinned + (below - off)
        };
        (row < body_area.height as usize).then_some(row as u16)
    };
    for (idx, mob_id) in foe_hits {
        if let Some(row) = screen_row(idx) {
            state.record_combat_hit(
                Rect {
                    x: body_area.x,
                    y: body_area.y + row,
                    width: body_area.width,
                    height: 1,
                },
                ClickAction::AttackMob(mob_id),
            );
        }
    }
    // Same for hostile adventurers in a pvp room's "Adventurers here" list.
    for (idx, target_id) in player_hits {
        if let Some(row) = screen_row(idx) {
            state.record_combat_hit(
                Rect {
                    x: body_area.x,
                    y: body_area.y + row,
                    width: body_area.width,
                    height: 1,
                },
                ClickAction::AttackPlayer(target_id),
            );
        }
    }
    frame.render_widget(Paragraph::new(lines), body_area);
    if let Some(rect) = footer_area {
        frame.render_widget(Paragraph::new(footer), rect);
    }
}

/// Titles panel: a selectable list of earned titles with their levels. Enter
/// sets the highlighted one as your displayed title (or clears it).
fn titles_panel(view: &PlayerView, cursor: usize) -> (Vec<Line<'static>>, Option<usize>) {
    let mut lines = vec![section("Titles")];
    let mut sel_line = None;
    if view.titles.is_empty() {
        lines.push(Line::from(Span::styled(
            "  none earned yet - slay notable foes",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    for (i, title) in view.titles.iter().enumerate() {
        let selected = i == cursor;
        if selected {
            sel_line = Some(lines.len());
        }
        let active = view.active_title == Some(i);
        let level = view.title_levels.get(i).copied().unwrap_or(1);
        let marker = if selected { ">" } else { " " };
        let active_tag = if active { " *" } else { "" };
        let style = if selected {
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .patch(theme::selection_style())
                .add_modifier(Modifier::BOLD)
        } else if active {
            Style::default()
                .fg(theme::BADGE_GOLD())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::BADGE_GOLD())
        };
        lines.push(Line::from(Span::styled(
            format!("{marker} Lv{level} {title}{active_tag}"),
            style,
        )));
    }
    lines.push(Line::raw(""));
    lines.push(hint("w/s", "select  Enter display"));
    lines.push(hint("k", "close  (* = shown by your name)"));
    (lines, sel_line)
}

// ---- The quest journal and the bounty board ------------------------------

/// Quest journal, in reading order: what the player is doing right now (the
/// starter step and accepted bounties), then the Long Road - the realm's spine
/// of great bosses - then the Frontier's zone quests once its gate titles are
/// held (sealed, they collapse to a single line instead of twenty rows of
/// endgame noise). A list panel: Enter on a row with a target tracks it on the
/// compass and world map.
fn quests_panel(
    view: &PlayerView,
    cursor: usize,
    tracked: Option<RoomId>,
) -> (Vec<Line<'static>>, Option<usize>) {
    let mut lines = vec![section("Quest Journal")];
    // Keys up top where they can't scroll away - the Long Road below makes
    // this the longest panel in the game.
    lines.push(hint("w/s", "move  Enter track on map  j close"));
    let mut sel_line = None;
    lines.push(section("In progress"));
    if view.quests.is_empty() {
        lines.push(Line::from(Span::styled(
            "  nothing underway - the boards in each capital post daily work",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    for (i, q) in view.quests.iter().enumerate() {
        let selected = i == cursor;
        if selected {
            sel_line = Some(lines.len());
        }
        let (mark, color) = if q.done {
            ("[x]", theme::SUCCESS())
        } else {
            ("[ ]", theme::AMBER())
        };
        let is_tracked = q.target.is_some() && q.target == tracked;
        let mut row_style = Style::default().fg(color);
        if selected {
            row_style = row_style
                .bg(theme::BG_SELECTION())
                .add_modifier(Modifier::BOLD);
        }
        let marker = if selected { ">" } else { " " };
        let mut spans = vec![Span::styled(
            format!("{marker}{mark} {}", q.name),
            row_style,
        )];
        if is_tracked {
            spans.push(Span::styled(
                " \u{2691} tracked".to_string(),
                Style::default().fg(theme::SUCCESS()),
            ));
        }
        lines.push(Line::from(spans));
        lines.push(Line::from(Span::styled(
            format!("    {}", q.desc),
            Style::default().fg(theme::TEXT_DIM()),
        )));
        if let Some(place) = quest_place_note(q.target, view) {
            lines.push(Line::from(Span::styled(
                format!("    {place}"),
                Style::default().fg(theme::AMBER_DIM()),
            )));
        }
        lines.push(Line::from(Span::styled(
            format!("    reward: {}", q.reward),
            Style::default().fg(theme::BADGE_GOLD()),
        )));
    }
    lines.push(Line::raw(""));

    lines.push(section("The Long Road"));
    lines.push(Line::from(Span::styled(
        "  every crown between you and the realm's end",
        Style::default().fg(theme::TEXT_DIM()),
    )));
    // The road rows continue the cursor list after the quests, so w/s can
    // walk (and scroll) the whole panel and Enter can track a crown's lair.
    for (ri, step) in view.road.iter().enumerate() {
        let selected = view.quests.len() + ri == cursor;
        if selected {
            sel_line = Some(lines.len());
        }
        let (mark, mut style) = if step.done {
            ("[x]", Style::default().fg(theme::SUCCESS()))
        } else if step.current {
            (
                "[>]",
                Style::default()
                    .fg(theme::AMBER_GLOW())
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            ("[ ]", Style::default().fg(theme::TEXT_DIM()))
        };
        if selected {
            style = style.bg(theme::BG_SELECTION()).add_modifier(Modifier::BOLD);
        }
        let marker = if selected { ">" } else { " " };
        let mut spans = vec![Span::styled(format!("{marker}{mark} {}", step.boss), style)];
        if step.target.is_some() && step.target == tracked {
            spans.push(Span::styled(
                " \u{2691} tracked".to_string(),
                Style::default().fg(theme::SUCCESS()),
            ));
        }
        lines.push(Line::from(spans));
        // Only the tracked crown carries the place note: it is the row where
        // "why is there no arrow on my map" actually gets asked, and nine of
        // these would bury the panel.
        if step.target.is_some()
            && step.target == tracked
            && let Some(place) = quest_place_note(step.target, view)
        {
            lines.push(Line::from(Span::styled(
                format!("    {place}"),
                Style::default().fg(theme::AMBER_DIM()),
            )));
        }
        let mut detail = format!("    {}", step.place);
        if !step.unlocks.is_empty() {
            detail.push_str(&format!(" - opens {}", step.unlocks));
        }
        lines.push(Line::from(Span::styled(
            detail,
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    lines.push(Line::from(Span::styled(
        "  side countries need no crown: the Sunderlakes (fishing), Broceliande \
         (taming), Aelunor, the Archipelago, the Wildbound Waste (pvp)",
        Style::default().fg(theme::TEXT_DIM()),
    )));
    lines.push(Line::raw(""));

    if view.frontier_open {
        let frontier_total = view
            .quests
            .iter()
            .filter(|q| q.kind == QuestKind::Frontier)
            .count();
        let done = view
            .quests
            .iter()
            .filter(|q| q.kind == QuestKind::Frontier && q.done)
            .count();
        lines.push(Line::from(Span::styled(
            format!("  Frontier: {done}/{frontier_total} zones cleared (listed above)"),
            Style::default().fg(theme::TEXT_DIM()),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "  The Frontier - sealed: it opens to the Archdemon's Bane bearing \
             all three living-dark seals",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    (lines, sel_line)
}

/// Render a full-screen column, skipping enough leading lines to keep the
/// selection (if any) in view. Lines are pre-wrapped by the caller, so one
/// logical line is one terminal row and the offset arithmetic is exact.
fn render_scrolled(frame: &mut Frame, rect: Rect, lines: Vec<Line<'static>>, sel: Option<usize>) {
    let h = rect.height as usize;
    let off = match sel {
        Some(s) if s + 3 > h => (s + 3 - h).min(lines.len()),
        _ => 0,
    };
    let shown: Vec<Line> = lines.into_iter().skip(off).collect();
    frame.render_widget(Paragraph::new(shown), rect);
}

/// Where a quest's target lies, for the journal: its region, and how it sits
/// relative to the player. Tracking any target aims the map's green arrow
/// along the walk there (`worldmap::track_aim`), so this line only has to say
/// how far that walk reaches: this floor, N floors away in the same land
/// (within `PAN_LIMIT`), or beyond this land. A land with no walking way in at
/// all gets no arrow, so it names the waystone instead.
fn quest_place_note(target: Option<RoomId>, view: &PlayerView) -> Option<String> {
    let target = target?;
    let (region, _) = super::world::region_atlas_entry(target)?;
    let coords = super::worldmap::world_coords();
    let floors = match (view.room.and_then(|r| coords.get(&r)), coords.get(&target)) {
        (Some(here), Some(there))
            if (here.x - there.x).abs() <= super::worldmap::PAN_LIMIT
                && (here.y - there.y).abs() <= super::worldmap::PAN_LIMIT =>
        {
            Some(there.z - here.z)
        }
        _ => None,
    };
    let portal_only = super::worldmap::portal_lands().contains(&region);
    Some(match (floors, portal_only) {
        (Some(0), _) => format!("in {region}"),
        (Some(dz), _) => {
            let n = dz.abs();
            format!(
                "in {region} - {n} floor{} {}",
                if n == 1 { "" } else { "s" },
                if dz < 0 { "down" } else { "up" },
            )
        }
        (None, true) => format!("in {region} - reached by waystone"),
        (None, false) => format!("in {region} - beyond this land"),
    })
}

/// One journal quest as its full-view rows: the name row (cursor and tracked
/// decoration included) with the wrapped description, where it lies, and the
/// reward under it.
fn quest_entry_rows(
    q: &QuestView,
    view: &PlayerView,
    selected: bool,
    tracked: bool,
    width: usize,
) -> Vec<Line<'static>> {
    let (mark, color) = if q.done {
        ("[x]", theme::SUCCESS())
    } else {
        ("[ ]", theme::AMBER())
    };
    let mut row_style = Style::default().fg(color);
    if selected {
        row_style = row_style
            .bg(theme::BG_SELECTION())
            .add_modifier(Modifier::BOLD);
    }
    let marker = if selected { ">" } else { " " };
    let mut spans = vec![Span::styled(
        format!("{marker}{mark} {}", q.name),
        row_style,
    )];
    if tracked {
        spans.push(Span::styled(
            " \u{2691} tracked".to_string(),
            Style::default().fg(theme::SUCCESS()),
        ));
    }
    let mut rows = vec![Line::from(spans)];
    rows.extend(side_text_wrap(&q.desc, theme::TEXT_DIM(), width));
    if let Some(place) = quest_place_note(q.target, view) {
        rows.extend(side_text_wrap(&place, theme::AMBER_DIM(), width));
    }
    rows.push(Line::from(Span::styled(
        format!("    reward: {}", q.reward),
        Style::default().fg(theme::BADGE_GOLD()),
    )));
    rows
}

/// The journal as a full view (wide terminals): the sidebar squeezes three
/// sections into one thin column; given room they become columns - active
/// work, the Long Road, the Frontier - with the same cursor, keys, and
/// tracking as the sidebar `quests_panel`, which still serves cramped
/// terminals.
fn draw_journal_screen(frame: &mut Frame, area: Rect, state: &State, view: &PlayerView) {
    let tracked = state.dest_room();
    let cursor = state.cursor();
    let rows = Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).split(area);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "Quest Journal",
                Style::default()
                    .fg(theme::AMBER_GLOW())
                    .add_modifier(Modifier::BOLD),
            )),
            hint("w/s", "move  Enter track on map  j close"),
            Line::raw(""),
        ]),
        rows[0],
    );
    let cols = Layout::horizontal([
        Constraint::Percentage(38),
        Constraint::Percentage(34),
        Constraint::Percentage(28),
    ])
    .split(rows[1]);

    // Column 1: active work - the starter step and accepted bounties.
    let w1 = (cols[0].width as usize).saturating_sub(2);
    let mut work: Vec<Line> = vec![section("In progress")];
    let mut work_sel = None;
    let mut any_active = false;
    for (i, q) in view.quests.iter().enumerate() {
        if q.kind == QuestKind::Frontier {
            continue;
        }
        any_active = true;
        let selected = i == cursor;
        if selected {
            work_sel = Some(work.len());
        }
        work.extend(quest_entry_rows(
            q,
            view,
            selected,
            q.target.is_some() && q.target == tracked,
            w1,
        ));
        work.push(Line::raw(""));
    }
    if !any_active {
        work.push(Line::from(Span::styled(
            "  nothing underway - the boards in each capital post daily work",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    render_scrolled(frame, cols[0], work, work_sel);

    // Column 2: the Long Road, with the ungated side countries below it.
    let w2 = (cols[1].width as usize).saturating_sub(2);
    let mut road: Vec<Line> = vec![section("The Long Road")];
    let mut road_sel = None;
    road.push(Line::from(Span::styled(
        "  every crown between you and the realm's end",
        Style::default().fg(theme::TEXT_DIM()),
    )));
    for (ri, step) in view.road.iter().enumerate() {
        let selected = view.quests.len() + ri == cursor;
        if selected {
            road_sel = Some(road.len());
        }
        let (mark, mut style) = if step.done {
            ("[x]", Style::default().fg(theme::SUCCESS()))
        } else if step.current {
            (
                "[>]",
                Style::default()
                    .fg(theme::AMBER_GLOW())
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            ("[ ]", Style::default().fg(theme::TEXT_DIM()))
        };
        if selected {
            style = style.bg(theme::BG_SELECTION()).add_modifier(Modifier::BOLD);
        }
        let marker = if selected { ">" } else { " " };
        let mut spans = vec![Span::styled(format!("{marker}{mark} {}", step.boss), style)];
        if step.target.is_some() && step.target == tracked {
            spans.push(Span::styled(
                " \u{2691} tracked".to_string(),
                Style::default().fg(theme::SUCCESS()),
            ));
        }
        road.push(Line::from(spans));
        // See the sidebar twin: only the tracked crown explains its arrow.
        if step.target.is_some()
            && step.target == tracked
            && let Some(place) = quest_place_note(step.target, view)
        {
            road.extend(side_text_wrap(&place, theme::AMBER_DIM(), w2));
        }
        let mut detail = step.place.to_string();
        if !step.unlocks.is_empty() {
            detail.push_str(&format!(" - opens {}", step.unlocks));
        }
        road.extend(side_text_wrap(&detail, theme::TEXT_DIM(), w2));
    }
    road.push(Line::raw(""));
    road.extend(side_text_wrap(
        "side countries need no crown: the Sunderlakes (fishing), Broceliande \
         (taming), Aelunor, the Archipelago, the Wildbound Waste (pvp)",
        theme::TEXT_DIM(),
        w2,
    ));
    render_scrolled(frame, cols[1], road, road_sel);

    // Column 3: the Frontier - its twenty zone quests once open, one sealed
    // line until then.
    let w3 = (cols[2].width as usize).saturating_sub(2);
    let mut frontier: Vec<Line> = vec![section("The Frontier")];
    let mut frontier_sel = None;
    if view.frontier_open {
        let total = view
            .quests
            .iter()
            .filter(|q| q.kind == QuestKind::Frontier)
            .count();
        let done = view
            .quests
            .iter()
            .filter(|q| q.kind == QuestKind::Frontier && q.done)
            .count();
        frontier.push(Line::from(Span::styled(
            format!("  {done}/{total} zones cleared"),
            Style::default().fg(theme::TEXT_DIM()),
        )));
        for (i, q) in view.quests.iter().enumerate() {
            if q.kind != QuestKind::Frontier {
                continue;
            }
            let selected = i == cursor;
            if selected {
                frontier_sel = Some(frontier.len());
            }
            frontier.extend(quest_entry_rows(
                q,
                view,
                selected,
                q.target.is_some() && q.target == tracked,
                w3,
            ));
        }
    } else {
        frontier.extend(side_text_wrap(
            "sealed: it opens to the Archdemon's Bane bearing all three \
             living-dark seals",
            theme::TEXT_DIM(),
            w3,
        ));
    }
    render_scrolled(frame, cols[2], frontier, frontier_sel);
}

/// The fixed cells of a full-screen item list row, sized once: deriving the
/// name width from a row's own price and tag lets a long price push the tag
/// off the column edge, which is how "▲+77%" rendered as "▲+7".
const ITEM_ROW_VALUE_W: usize = 9;
const ITEM_ROW_TAG_W: usize = 7;

/// The upgrade tag cell of an item row: "▲+18%" green, "▼-12%" red, blank
/// when there is nothing to compare.
fn upgrade_tag(compare_pct: Option<i32>) -> (String, Color) {
    match compare_pct {
        Some(pct) if pct > 0 => (format!("\u{25B2}{pct:+}%"), theme::SUCCESS()),
        Some(pct) if pct < 0 => (format!("\u{25BC}{pct:+}%"), theme::ERROR()),
        Some(pct) => (format!("={pct:+}%"), theme::TEXT_DIM()),
        None => (String::new(), theme::TEXT_DIM()),
    }
}

/// One line of a full-screen item list (the shop's stock, the pack): the name
/// in its rarity colour, a money cell, and a tag cell, all at fixed widths so
/// the columns line up and nothing is clipped at the edge.
fn item_row_line(
    name: &str,
    rarity: &str,
    selected: bool,
    value: (String, Color),
    tag: (String, Color),
    list_w: usize,
) -> Line<'static> {
    let marker = if selected { "> " } else { "  " };
    let name_w = list_w.saturating_sub(marker.len() + ITEM_ROW_VALUE_W + ITEM_ROW_TAG_W);
    let name_style = {
        let base = Style::default().fg(rarity_color(rarity));
        if selected {
            base.patch(theme::selection_style())
                .add_modifier(Modifier::BOLD)
        } else {
            base
        }
    };
    let (value, value_color) = value;
    let (tag, tag_color) = tag;
    Line::from(vec![
        Span::styled(
            format!("{marker}{:<name_w$}", truncate_chars(name, name_w)),
            name_style,
        ),
        Span::styled(
            format!("{value:>ITEM_ROW_VALUE_W$}"),
            Style::default().fg(value_color),
        ),
        Span::styled(
            format!("{tag:>ITEM_ROW_TAG_W$}"),
            Style::default().fg(tag_color).add_modifier(Modifier::BOLD),
        ),
    ])
}

/// The page header of a full-screen item view: title, who/what and the purse,
/// then one or two key-hint lines.
fn item_screen_header(
    title: &str,
    subtitle: &str,
    hints: Vec<Line<'static>>,
) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(vec![
        Span::styled(
            title.to_string(),
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("   {subtitle}"),
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ])];
    lines.extend(hints);
    lines.push(Line::raw(""));
    lines
}

fn purse_label(gold: i64, banked_gold: i64) -> String {
    if banked_gold > 0 {
        format!("your gold: {gold}  (bank: {banked_gold})")
    } else {
        format!("your gold: {gold}")
    }
}

/// A piece of gear as the detail pane of a full-screen item view shows it.
struct ItemDetail<'a> {
    name: &'a str,
    rarity: &'a str,
    /// "worn on the weapon" / "you are wearing this (weapon)": the pane's
    /// second line, None for non-gear.
    slot_line: Option<String>,
    stats: &'a str,
    /// Whether to stand the piece against what is worn in its slot. Off for
    /// the worn piece itself.
    compare_to_worn: bool,
    slot: Option<&'a str>,
    worn_name: Option<&'a str>,
    worn_stats: Option<&'a str>,
    compare: &'a str,
    compare_pct: Option<i32>,
    desc: &'a str,
}

/// The body of a detail pane: the piece, what it would replace, the delta,
/// and its description. The caller appends its own action prompt.
fn item_detail_lines(d: &ItemDetail, width: usize) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(vec![
        Span::styled(
            d.name.to_string(),
            Style::default()
                .fg(rarity_color(d.rarity))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  {}", d.rarity),
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ])];
    if let Some(slot_line) = &d.slot_line {
        lines.push(Line::from(Span::styled(
            format!("  {slot_line}"),
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    lines.push(Line::raw(""));
    if !d.stats.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("  {}", d.stats),
            Style::default().fg(theme::AMBER()),
        )));
        lines.push(Line::raw(""));
    }

    // The direct comparison: what is on your body right now, then the delta
    // between the two, so the trade reads without arithmetic.
    if d.compare_to_worn {
        match (d.worn_name, d.worn_stats, d.slot) {
            (Some(name), Some(stats), _) => {
                lines.push(Line::from(Span::styled(
                    "  instead of what you wear:",
                    Style::default().fg(theme::TEXT_DIM()),
                )));
                lines.push(Line::from(Span::styled(
                    format!("  {name}"),
                    Style::default().fg(theme::TEXT_BRIGHT()),
                )));
                lines.push(Line::from(Span::styled(
                    format!("  {stats}"),
                    Style::default().fg(theme::TEXT_DIM()),
                )));
            }
            // No rival and no delta: the slot already holds this very piece.
            (_, _, Some(_)) if d.compare.is_empty() => lines.push(Line::from(Span::styled(
                "  you already wear one",
                Style::default().fg(theme::TEXT_DIM()),
            ))),
            (_, _, Some(_)) => lines.push(Line::from(Span::styled(
                "  that slot is empty",
                Style::default().fg(theme::TEXT_DIM()),
            ))),
            (_, _, None) => {}
        }
        if let Some(line) = compare_line(d.compare) {
            lines.push(line);
        }
        if let Some(pct) = d.compare_pct {
            let (tag, color) = upgrade_tag(Some(pct));
            lines.push(Line::from(Span::styled(
                format!("  {tag} overall"),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            )));
        }
        lines.push(Line::raw(""));
    }
    if !d.desc.is_empty() {
        lines.extend(side_text_wrap(d.desc, LAT_TEXT, width));
        lines.push(Line::raw(""));
    }
    lines
}

fn prompt_line(text: String, color: Color) -> Line<'static> {
    Line::from(Span::styled(
        format!("  {text}"),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    ))
}

/// The shop as a full screen: a dense one-line-per-item list on the left,
/// grouped under the same collapsible category headers the side panel uses, and
/// the highlighted piece stood next to what it would replace on the right.
///
/// The side panel renders the same stock as four wrapped lines an item (name,
/// stats, comparison, description), which a ten-item shop already outgrows and
/// the market tier's stock outgrows badly. Same cursor, same rows, same keys:
/// only the shape changes, so `w`/`s`, Enter, and the collapse toggle behave
/// identically in both renderings.
///
/// Takes its data rather than the `State` it is drawn from, so the layout can
/// be rendered in a test without standing up a service.
fn draw_shop_screen(
    frame: &mut Frame,
    area: Rect,
    shop_rows: &[SectionRow],
    shop: &ShopView,
    cursor: usize,
    gold: i64,
    banked_gold: i64,
) {
    let rows = Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).split(area);
    frame.render_widget(
        Paragraph::new(item_screen_header(
            &shop.shop_name,
            &format!("{} - {}", shop.npc_name, purse_label(gold, banked_gold)),
            vec![hint("w/s", "select  Enter buy/fold  b back")],
        )),
        rows[0],
    );

    let cols =
        Layout::horizontal([Constraint::Percentage(52), Constraint::Percentage(48)]).split(rows[1]);

    // Left: one line an item - name, price, and the upgrade tag. Everything
    // else moves to the detail pane, so the list stays scannable.
    let list_w = cols[0].width as usize;
    let mut list: Vec<Line> = Vec::new();
    let mut sel = None;
    for (i, row) in shop_rows.iter().enumerate() {
        let selected = i == cursor;
        if selected {
            sel = Some(list.len());
        }
        match row {
            SectionRow::Header {
                label,
                count,
                collapsed,
                ..
            } => list.push(section_header_line(label, *count, *collapsed, selected)),
            SectionRow::Item { index } => {
                let Some(e) = shop.entries.get(*index) else {
                    continue;
                };
                let price_color = if e.affordable {
                    theme::BADGE_GOLD()
                } else {
                    theme::ERROR()
                };
                list.push(item_row_line(
                    &e.name,
                    &e.rarity,
                    selected,
                    (format!("{}g", e.price), price_color),
                    upgrade_tag(e.compare_pct),
                    list_w,
                ));
            }
        }
    }
    render_scrolled(frame, cols[0], list, sel);

    // Right: the highlighted piece against what it would replace.
    let detail_w = (cols[1].width as usize).saturating_sub(2);
    let entry = shop_rows.get(cursor).and_then(|row| match row {
        SectionRow::Item { index } => shop.entries.get(*index),
        SectionRow::Header { .. } => None,
    });
    let detail = match entry {
        None => vec![Line::from(Span::styled(
            "  A category. Enter folds it; w/s moves on.",
            Style::default().fg(theme::TEXT_DIM()),
        ))],
        Some(e) => {
            let mut detail = item_detail_lines(
                &ItemDetail {
                    name: &e.name,
                    rarity: &e.rarity,
                    slot_line: e.slot.as_ref().map(|slot| format!("worn on the {slot}")),
                    stats: &e.stats,
                    compare_to_worn: true,
                    slot: e.slot.as_deref(),
                    worn_name: e.worn_name.as_deref(),
                    worn_stats: e.worn_stats.as_deref(),
                    compare: &e.compare,
                    compare_pct: e.compare_pct,
                    desc: e.desc,
                },
                detail_w,
            );
            detail.push(if e.affordable {
                prompt_line(format!("Enter to buy - {}g", e.price), theme::BADGE_GOLD())
            } else {
                prompt_line(
                    format!("{}g - you cannot afford this", e.price),
                    theme::ERROR(),
                )
            });
            detail
        }
    };
    frame.render_widget(Paragraph::new(detail), cols[1]);
}

/// The pack covers the field, and it is the panel opened mid-fight to drink
/// something, so its header keeps the vitals and the fight in sight.
fn pack_vitals_spans(view: &PlayerView) -> Vec<Span<'static>> {
    let mut spans = vec![
        Span::styled(
            format!("  HP {}/{}", view.hp, view.max_hp),
            Style::default()
                .fg(hp_color(view.hp, view.max_hp))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                "  {} {}/{}",
                view.resource_name, view.resource, view.max_resource
            ),
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ];
    if let Some(foe) = &view.in_combat_with {
        spans.push(Span::styled(
            format!("  fighting {foe}"),
            Style::default()
                .fg(theme::ERROR())
                .add_modifier(Modifier::BOLD),
        ));
    }
    spans
}

/// The pack as a full screen, the same shape as the shop: every carried and
/// worn piece on one line (name, sell value, the upgrade tag or `worn`) under
/// the collapsible category headers, and the highlighted piece on the right,
/// stood against what it would replace, with what Enter and `x` will do to it.
///
/// The side `inventory_panel` wraps each piece into a four-line stanza, which
/// a pack of loot turns into a long scroll with no way to compare anything.
/// Same rows, cursor, and keys in both renderings.
///
/// A shop in the room decides whether the sell keys are offered: `x` and
/// `A/C/J` only do anything at a merchant.
fn draw_inventory_screen(
    frame: &mut Frame,
    area: Rect,
    inv_rows: &[SectionRow],
    view: &PlayerView,
    cursor: usize,
) {
    let inventory: &[InvView] = &view.inventory;
    let (gold, banked_gold) = (view.gold, view.banked_gold);
    let merchant_here = view.shop.is_some();
    let rows = Layout::vertical([Constraint::Length(4), Constraint::Min(1)]).split(area);
    let carried = inventory.iter().filter(|it| !it.equipped).count();
    let worn = inventory.len() - carried;
    let sell_hint = if merchant_here {
        hint("x", "sell one  A/C/J sell all / commons / non-upgrades")
    } else {
        hint("x A/C/J", "sell, once you stand at a merchant")
    };
    let mut header = item_screen_header(
        "Inventory",
        &format!(
            "{carried} carried, {worn} worn - {}",
            purse_label(gold, banked_gold)
        ),
        vec![
            hint("w/s", "select  Enter equip/use/fold  t back"),
            sell_hint,
        ],
    );
    // On the title line, ahead of the counts: no extra row at 100x20, and a
    // long line clips the purse rather than the fight.
    header[0].spans.splice(1..1, pack_vitals_spans(view));
    frame.render_widget(Paragraph::new(header), rows[0]);

    let cols =
        Layout::horizontal([Constraint::Percentage(52), Constraint::Percentage(48)]).split(rows[1]);

    // Left: one line a piece - name, what it sells for, and the upgrade tag
    // (or `worn`, for what is already on your body).
    let list_w = cols[0].width as usize;
    let mut list: Vec<Line> = Vec::new();
    let mut sel = None;
    if inventory.is_empty() {
        list.push(Line::from(Span::styled(
            "  Your pack is empty and you wear nothing.",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    for (i, row) in inv_rows.iter().enumerate() {
        let selected = i == cursor;
        if selected {
            sel = Some(list.len());
        }
        match row {
            SectionRow::Header {
                label,
                count,
                collapsed,
                ..
            } => list.push(section_header_line(label, *count, *collapsed, selected)),
            SectionRow::Item { index } => {
                let Some(it) = inventory.get(*index) else {
                    continue;
                };
                let value_color = if merchant_here && !it.equipped {
                    theme::BADGE_GOLD()
                } else {
                    theme::TEXT_DIM()
                };
                let tag = if it.equipped {
                    ("worn".to_string(), theme::AMBER())
                } else {
                    upgrade_tag(it.compare_pct)
                };
                list.push(item_row_line(
                    &it.name,
                    &it.rarity,
                    selected,
                    (format!("{}g", it.sell_price), value_color),
                    tag,
                    list_w,
                ));
            }
        }
    }
    render_scrolled(frame, cols[0], list, sel);

    // Right: the highlighted piece, and what the keys will do to it.
    let detail_w = (cols[1].width as usize).saturating_sub(2);
    let entry = inv_rows.get(cursor).and_then(|row| match row {
        SectionRow::Item { index } => inventory.get(*index),
        SectionRow::Header { .. } => None,
    });
    let detail = match entry {
        None if inventory.is_empty() => Vec::new(),
        None => vec![Line::from(Span::styled(
            "  A category. Enter folds it; w/s moves on.",
            Style::default().fg(theme::TEXT_DIM()),
        ))],
        Some(it) => {
            let slot_line = it.slot.as_ref().map(|slot| match it.equipped {
                true => format!("you are wearing this ({slot})"),
                false => format!("worn on the {slot}"),
            });
            let mut detail = item_detail_lines(
                &ItemDetail {
                    name: &it.name,
                    rarity: &it.rarity,
                    slot_line,
                    stats: &it.stats,
                    compare_to_worn: !it.equipped,
                    slot: it.slot.as_deref(),
                    worn_name: it.worn_name.as_deref(),
                    worn_stats: it.worn_stats.as_deref(),
                    compare: &it.compare,
                    compare_pct: it.compare_pct,
                    desc: it.desc,
                },
                detail_w,
            );
            let valuable = it.category == "Valuables";
            match inv_action(it) {
                InvAction::Unequip => detail.push(prompt_line(
                    "Enter to take it off".to_string(),
                    theme::AMBER(),
                )),
                InvAction::Equip => detail.push(prompt_line(
                    "Enter to put it on".to_string(),
                    theme::SUCCESS(),
                )),
                InvAction::Use if valuable => detail.push(prompt_line(
                    "a valuable: its worth is in the selling".to_string(),
                    theme::TEXT_DIM(),
                )),
                InvAction::Use => {
                    detail.push(prompt_line("Enter to use it".to_string(), theme::SUCCESS()))
                }
            }
            detail.push(match (it.equipped, merchant_here) {
                (true, true) => prompt_line(
                    "take it off before you sell it".to_string(),
                    theme::TEXT_DIM(),
                ),
                (true, false) => prompt_line(
                    format!("worth {}g to a merchant", it.sell_price),
                    theme::TEXT_DIM(),
                ),
                (false, true) => prompt_line(
                    format!("x to sell - {}g", it.sell_price),
                    theme::BADGE_GOLD(),
                ),
                (false, false) => prompt_line(
                    format!("sells for {}g at a merchant", it.sell_price),
                    theme::TEXT_DIM(),
                ),
            });
            detail
        }
    };
    frame.render_widget(Paragraph::new(detail), cols[1]);
}

/// What the list column's money cell says for a recipe. Crafting has no price,
/// and the cell it would sit in is the one thing a maker scanning the list
/// actually wants: can I make this right now, and if not, what stops me.
fn craft_status_cell(e: &super::svc::CraftEntryView) -> (String, Color) {
    if e.craftable {
        return ("ready".to_string(), theme::SUCCESS());
    }
    if e.skill_level < e.level_req {
        // A hard gate: no amount of gathering opens it today.
        return (format!("lvl {}", e.level_req), theme::ERROR());
    }
    // Soft: how many of the ingredient lines are covered. "1/2" reads as
    // "go and get the other one", which "need materials" never did.
    let held = e.ingredients.iter().filter(|i| i.have >= i.need).count();
    (format!("{held}/{}", e.ingredients.len()), theme::AMBER())
}

/// The ingredient checklist of the detail pane: every line of the recipe with
/// what is in the pack against what it costs. The sidebar panel collapses this
/// to one "need materials" string, which names neither the material nor the
/// shortfall.
fn craft_material_lines(e: &super::svc::CraftEntryView) -> Vec<Line<'static>> {
    let mut lines = vec![section("Materials")];
    for ing in &e.ingredients {
        let enough = ing.have >= ing.need;
        let (mark, color) = if enough {
            ("\u{2713}", theme::SUCCESS())
        } else {
            ("\u{2717}", theme::ERROR())
        };
        lines.push(Line::from(vec![
            Span::styled(format!("  {mark} "), Style::default().fg(color)),
            Span::styled(
                format!("{}x {}", ing.need, ing.name),
                Style::default().fg(if enough {
                    theme::TEXT()
                } else {
                    theme::TEXT_BRIGHT()
                }),
            ),
            Span::styled(
                format!("   you have {}", ing.have),
                Style::default().fg(if enough { theme::TEXT_DIM() } else { color }),
            ),
        ]));
    }
    lines
}

/// The trade line: where the maker stands in this skill, what the recipe asks
/// of it, and what making one is worth in xp.
fn craft_skill_line(e: &super::svc::CraftEntryView) -> Line<'static> {
    let gated = e.skill_level < e.level_req;
    Line::from(vec![
        Span::styled(
            format!("  {} {}", e.skill, e.skill_level),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  \u{00b7}  needs {}", e.level_req),
            Style::default().fg(if gated {
                theme::ERROR()
            } else {
                theme::TEXT_DIM()
            }),
        ),
        Span::styled(
            format!("  \u{00b7}  +{} xp", e.xp),
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ])
}

/// The craft station as a full screen, the shop's and the pack's third twin:
/// every recipe worked here on one line (name, whether it can be made, the
/// upgrade tag) under the collapsible skill headers, and the highlighted recipe
/// on the right - the piece it makes stood against what is worn in its slot,
/// the ingredient checklist with what is actually in the pack, and the trade
/// gate.
///
/// The sidebar `crafting_panel` renders each recipe as a name row plus a
/// wrapped ingredient row and no comparison at all, so a forge's stock of
/// recipes becomes a scroll with nothing in it to decide on: a maker could not
/// see whether the sword was better than the one on their hip, nor which of the
/// four materials they were short of. Same rows, same cursor, same keys in both
/// renderings; only the shape changes.
///
/// Takes its data rather than the `State` it is drawn from, so the layout can
/// be rendered in a test without standing up a service.
fn draw_craft_screen(
    frame: &mut Frame,
    area: Rect,
    craft_rows: &[SectionRow],
    craft: &CraftView,
    cursor: usize,
) {
    let rows = Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).split(area);

    // The subtitle carries the trades themselves, not just the furniture: at a
    // forge, "Smithing 12" is the number that decides what is on this list.
    let mut trades: Vec<String> = Vec::new();
    for e in &craft.entries {
        let label = format!("{} {}", e.skill, e.skill_level);
        if !trades.contains(&label) {
            trades.push(label);
        }
    }
    let subtitle = match trades.is_empty() {
        true => craft.stations.clone(),
        false => format!("{} - {}", craft.stations, trades.join(", ")),
    };
    frame.render_widget(
        Paragraph::new(item_screen_header(
            "Crafting",
            &subtitle,
            vec![hint("w/s", "select  Enter craft/fold  u back")],
        )),
        rows[0],
    );

    let cols =
        Layout::horizontal([Constraint::Percentage(52), Constraint::Percentage(48)]).split(rows[1]);

    // Left: one line a recipe - what it makes, whether it can be made, and
    // whether it beats what is worn.
    let list_w = cols[0].width as usize;
    let mut list: Vec<Line> = Vec::new();
    let mut sel = None;
    if craft.entries.is_empty() {
        list.push(Line::from(Span::styled(
            "  Nothing is worked at this station.",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    for (i, row) in craft_rows.iter().enumerate() {
        let selected = i == cursor;
        if selected {
            sel = Some(list.len());
        }
        match row {
            SectionRow::Header {
                label,
                count,
                collapsed,
                ..
            } => list.push(section_header_line(label, *count, *collapsed, selected)),
            SectionRow::Item { index } => {
                let Some(e) = craft.entries.get(*index) else {
                    continue;
                };
                let name = match e.qty > 1 {
                    true => format!("{}x {}", e.qty, e.name),
                    false => e.name.clone(),
                };
                list.push(item_row_line(
                    &name,
                    &e.rarity,
                    selected,
                    craft_status_cell(e),
                    upgrade_tag(e.compare_pct),
                    list_w,
                ));
            }
        }
    }
    render_scrolled(frame, cols[0], list, sel);

    // Right: what the highlighted recipe makes, what it costs, and what stands
    // between the maker and it.
    let detail_w = (cols[1].width as usize).saturating_sub(2);
    let entry = craft_rows.get(cursor).and_then(|row| match row {
        SectionRow::Item { index } => craft.entries.get(*index),
        SectionRow::Header { .. } => None,
    });
    let detail = match entry {
        None if craft.entries.is_empty() => Vec::new(),
        None => vec![Line::from(Span::styled(
            "  A trade. Enter folds it; w/s moves on.",
            Style::default().fg(theme::TEXT_DIM()),
        ))],
        Some(e) => {
            let mut detail = item_detail_lines(
                &ItemDetail {
                    name: &e.name,
                    rarity: &e.rarity,
                    slot_line: e.slot.as_ref().map(|slot| format!("worn on the {slot}")),
                    stats: &e.stats,
                    // Only gear has a rival on your body; a draught or an
                    // ingot has nothing to be stood against.
                    compare_to_worn: e.slot.is_some(),
                    slot: e.slot.as_deref(),
                    worn_name: e.worn_name.as_deref(),
                    worn_stats: e.worn_stats.as_deref(),
                    compare: &e.compare,
                    compare_pct: e.compare_pct,
                    desc: e.desc,
                },
                detail_w,
            );
            detail.extend(craft_material_lines(e));
            detail.push(Line::raw(""));
            detail.push(craft_skill_line(e));
            detail.push(Line::raw(""));
            detail.push(match (e.craftable, e.skill_level < e.level_req) {
                (true, _) => prompt_line("Enter to craft it".to_string(), theme::SUCCESS()),
                (false, true) => prompt_line(
                    format!(
                        "{} {} first - you are {}",
                        e.skill, e.level_req, e.skill_level
                    ),
                    theme::ERROR(),
                ),
                (false, false) => prompt_line(
                    "gather the missing materials first".to_string(),
                    theme::AMBER(),
                ),
            });
            detail
        }
    };
    frame.render_widget(Paragraph::new(detail), cols[1]);
}

/// The board as a full view (wide terminals): a master-detail split - the
/// postings list on the left, the highlighted posting's full story on the
/// right. Same cursor and keys as the sidebar `board_panel`, which still
/// serves cramped terminals.
fn draw_board_screen(frame: &mut Frame, area: Rect, state: &State, view: &PlayerView) {
    let Some(board) = &view.board else {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "No board here.",
                Style::default().fg(theme::TEXT_DIM()),
            ))),
            area,
        );
        return;
    };
    let cursor = state.cursor();
    let rows = Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).split(area);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "Quest Board",
                Style::default()
                    .fg(theme::AMBER_GLOW())
                    .add_modifier(Modifier::BOLD),
            )),
            hint("w/s", "select  Enter claim (READY) / accept  o back"),
            Line::raw(""),
        ]),
        rows[0],
    );
    if board.entries.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "No bounties posted right now. Make progress on what you're \
                 already carrying, or come back later.",
                Style::default().fg(theme::TEXT_DIM()),
            ))),
            rows[1],
        );
        return;
    }
    let cols =
        Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)]).split(rows[1]);

    // Left: the postings, one row each.
    let list_w = cols[0].width as usize;
    let mut list: Vec<Line> = Vec::new();
    let mut sel = None;
    for (i, e) in board.entries.iter().enumerate() {
        let selected = i == cursor;
        if selected {
            sel = Some(list.len());
        }
        let (tag, tag_color) = if e.ready {
            ("READY", theme::SUCCESS())
        } else if e.locked {
            ("sealed", theme::ERROR())
        } else {
            ("~Lv", theme::AMBER_DIM())
        };
        let tag_text = if tag == "~Lv" {
            format!("  ~Lv{}", e.suggested_level)
        } else {
            format!("  [{tag}]")
        };
        let base_fg = if e.locked {
            theme::TEXT_DIM()
        } else {
            theme::TEXT_BRIGHT()
        };
        let name_style = if selected {
            Style::default()
                .fg(base_fg)
                .bg(theme::BG_SELECTION())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(base_fg)
        };
        let marker = if selected { "> " } else { "  " };
        let name_w = list_w.saturating_sub(marker.len() + tag_text.chars().count() + 1);
        list.push(Line::from(vec![
            Span::styled(
                format!("{marker}{}", truncate_chars(&e.title, name_w)),
                name_style,
            ),
            Span::styled(tag_text, Style::default().fg(tag_color)),
        ]));
    }
    render_scrolled(frame, cols[0], list, sel);

    // Right: the highlighted posting in full.
    let detail_w = (cols[1].width as usize).saturating_sub(2);
    let mut detail: Vec<Line> = Vec::new();
    if let Some(e) = board.entries.get(cursor) {
        detail.push(Line::from(Span::styled(
            e.title.clone(),
            Style::default()
                .fg(if e.locked {
                    theme::TEXT_DIM()
                } else {
                    theme::TEXT_BRIGHT()
                })
                .add_modifier(Modifier::BOLD),
        )));
        detail.push(Line::from(Span::styled(
            format!("  a fair fight around Lv{}", e.suggested_level),
            Style::default().fg(theme::AMBER_DIM()),
        )));
        detail.push(Line::raw(""));
        detail.extend(side_text_wrap(&e.blurb, LAT_TEXT, detail_w));
        detail.push(Line::raw(""));
        detail.extend(side_text_wrap(
            &format!("task: {}", e.objective),
            theme::AMBER(),
            detail_w,
        ));
        detail.extend(side_text_wrap(&e.hint, theme::TEXT_DIM(), detail_w));
        detail.push(Line::raw(""));
        detail.push(Line::from(Span::styled(
            format!("  reward: {}", e.reward),
            Style::default().fg(theme::BADGE_GOLD()),
        )));
        if e.ready {
            detail.push(Line::from(Span::styled(
                "  READY - Enter turns it in",
                Style::default().fg(theme::SUCCESS()),
            )));
        } else if e.locked {
            detail.extend(side_text_wrap(
                "sealed - the ground this names refuses you at the door; its \
                 gate opens further down the Long Road (j)",
                theme::ERROR(),
                detail_w,
            ));
        }
    }
    frame.render_widget(Paragraph::new(detail), cols[1]);
}

// ---- The leaderboard, vitals, and the room / battle panels ---------------

/// One leaderboard row: rank, level + class abbreviation, name, then the
/// board's own value column (already formatted by the caller, since its
/// meaning - a bare level, a kill count, a gold total - differs per board).
fn leaderboard_row(
    rank: usize,
    e: &LeaderboardEntry,
    usernames: &UsernameLookup<'_>,
    value: &str,
) -> Line<'static> {
    let name = usernames
        .get(&e.user_id)
        .cloned()
        .unwrap_or_else(|| "adventurer".to_string());
    let abbrev = class_abbrev(&e.class_key);
    Line::from(Span::styled(
        format!(
            "  {rank:>2}. Lv{:<3} {abbrev:<3} {name:<16} {value}",
            e.level
        ),
        Style::default().fg(if rank == 1 {
            theme::BADGE_GOLD()
        } else {
            theme::TEXT_BRIGHT()
        }),
    ))
}

fn leaderboard_panel(view: &PlayerView, usernames: &UsernameLookup<'_>) -> Vec<Line<'static>> {
    let mut lines = vec![section("Leaderboard")];
    lines.push(Line::from(Span::styled(
        "  the ten sharpest adventurers online right now",
        Style::default().fg(theme::TEXT_DIM()),
    )));
    lines.push(Line::raw(""));

    lines.push(section("By Level"));
    if view.leaderboard.by_level.is_empty() {
        lines.push(Line::from(Span::styled(
            "  no one else is online yet",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    for (i, e) in view.leaderboard.by_level.iter().enumerate() {
        lines.push(leaderboard_row(i + 1, e, usernames, ""));
    }
    lines.push(Line::raw(""));

    lines.push(section("By PvP Kills (the Wildbound Waste)"));
    if view.leaderboard.by_pvp_kills.iter().all(|e| e.value == 0) {
        lines.push(Line::from(Span::styled(
            "  no rivals slain yet - the Waste awaits",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    } else {
        for (i, e) in view.leaderboard.by_pvp_kills.iter().enumerate() {
            if e.value == 0 {
                break;
            }
            lines.push(leaderboard_row(
                i + 1,
                e,
                usernames,
                &format!("{} kill{}", e.value, if e.value == 1 { "" } else { "s" }),
            ));
        }
    }
    lines.push(Line::raw(""));

    lines.push(section("By Gold"));
    for (i, e) in view.leaderboard.by_gold.iter().enumerate() {
        lines.push(leaderboard_row(
            i + 1,
            e,
            usernames,
            &format!("{}g", e.value),
        ));
    }
    lines.push(Line::raw(""));

    lines.push(hint("!", "close"));
    lines.push(hint("[ ]", "scroll"));
    lines
}

/// How the vitals block renders HP and the class resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VitalStyle {
    /// Compact numbers, for the peaceful panels: nothing is swinging back, so
    /// there is nothing to judge at a glance.
    Numbers,
    /// Wide meters sized to the given panel width, for the battle frame - the
    /// same shape the foe's bar uses, so both sides of the fight read the same
    /// way and "can I out-trade this" is a look, not arithmetic.
    Meters(usize),
}

fn vitals(view: &PlayerView, style: VitalStyle) -> Vec<Line<'static>> {
    let hp_fg = hp_color(view.hp, view.max_hp);
    let (hp_line, resource_line) = match style {
        VitalStyle::Numbers => (
            Line::from(vec![
                Span::styled(vital_label("HP"), Style::default().fg(theme::TEXT_DIM())),
                Span::styled(
                    format!("{}/{}", view.hp, view.max_hp),
                    Style::default().fg(hp_fg).add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled(
                    vital_label(&short_res(&view.resource_name)),
                    Style::default().fg(theme::TEXT_DIM()),
                ),
                Span::styled(
                    format!("{}/{}", view.resource, view.max_resource),
                    Style::default().fg(theme::MENTION()),
                ),
            ]),
        ),
        VitalStyle::Meters(width) => (
            panel_meter_line(&vital_label("HP"), view.hp, view.max_hp, hp_fg, width),
            panel_meter_line(
                &vital_label(&short_res(&view.resource_name)),
                view.resource,
                view.max_resource,
                theme::MENTION(),
                width,
            ),
        ),
    };
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                format!("{} ", view.class_name),
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("lvl {}", view.level),
                Style::default().fg(theme::TEXT_BRIGHT()),
            ),
            Span::styled(
                match view.active_title.and_then(|i| view.titles.get(i)) {
                    Some(title) => format!("  {title}"),
                    None => String::new(),
                },
                Style::default().fg(theme::BADGE_GOLD()),
            ),
        ]),
        hp_line,
        resource_line,
        Line::from(vec![
            Span::styled(vital_label("gold"), Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                format!("{}", view.gold),
                Style::default().fg(theme::BADGE_GOLD()),
            ),
        ]),
    ];
    if view.banked_gold > 0 {
        lines.push(Line::from(vec![
            Span::styled(vital_label("bank"), Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                format!("{}", view.banked_gold),
                Style::default().fg(theme::TEXT_BRIGHT()),
            ),
        ]));
    }
    lines
}

/// The room panel, split where it is anchored: `body` scrolls in the top of the
/// rail, `footer` is pinned to its floor. `foe_hits`/`player_hits` index into
/// `body`.
struct RoomPanel {
    body: Vec<Line<'static>>,
    footer: Vec<Line<'static>>,
    foe_hits: Vec<(usize, u32)>,
    player_hits: Vec<(usize, Uuid)>,
}

/// The room side panel. Alongside the lines it records, for each foe, the line
/// index of its roster row and its spawn id, so the caller can record a
/// clickable rect over each foe (click a foe to lock onto it).
fn room_panel(
    view: &PlayerView,
    usernames: &UsernameLookup<'_>,
    width: usize,
    heading: Option<Heading>,
) -> RoomPanel {
    let mut foe_hits: Vec<(usize, u32)> = Vec::new();
    let mut player_hits: Vec<(usize, Uuid)> = Vec::new();
    let mut lines = vitals(view, VitalStyle::Numbers);
    lines.push(Line::raw(""));
    // What this room lets you do, before anything you have to read: a shop or a
    // stable is the reason you walked in here.
    lines.extend(room_actions(view));
    lines.push(section("Here"));
    // The zone plus its level band, so one glance answers "do I belong here".
    lines.extend(side_text_wrap(&zone_with_band(view), LAT_TEXT, width));
    // The living-world clock: time of day and weather. A phase glyph plus a
    // danger colour during dusk/night (when mobs hit 25% harder) makes the
    // clock legible at a glance instead of reading as pure flavour text.
    let clock_color = if view.time_of_day_dark {
        theme::ERROR()
    } else {
        theme::AMBER_DIM()
    };
    lines.push(Line::from(Span::styled(
        format!(
            "  {} {} · {}",
            view.time_of_day_glyph, view.time_of_day, view.weather
        ),
        Style::default().fg(clock_color),
    )));
    // An active escort: who you're leading, their health, and where to.
    if let Some((name, hp, max_hp, dest)) = &view.escort {
        lines.push(Line::from(vec![
            Span::styled(
                format!("  ★ {name} "),
                Style::default()
                    .fg(theme::MENTION())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{hp}/{max_hp}"),
                Style::default().fg(hp_color(*hp, *max_hp)),
            ),
        ]));
        lines.push(Line::from(Span::styled(
            format!("    lead to {dest}"),
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    // Your companion, if any: glyph, name, level, and health.
    if let Some(pet) = &view.pet {
        let name_color = if pet.downed {
            theme::ERROR()
        } else {
            theme::SUCCESS()
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {} {} ", pet.glyph, pet.name),
                Style::default().fg(name_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                if pet.downed {
                    "downed".to_string()
                } else {
                    format!("Lv{} {}/{}", pet.level, pet.hp, pet.max_hp)
                },
                Style::default().fg(if pet.downed {
                    theme::ERROR()
                } else {
                    hp_color(pet.hp, pet.max_hp)
                }),
            ),
        ]));
        // The companion's unlocked auto-skills (fire automatically in combat).
        if !pet.skills.is_empty() {
            let names = pet
                .skills
                .iter()
                .map(|(n, _)| n.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            lines.extend(side_text_wrap(
                &format!("    skills: {names}"),
                theme::AMBER_DIM(),
                width,
            ));
        }
        for chip_line in pack_hint_chips(
            &pet_feed_chips(pet)
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            width.saturating_sub(2),
        ) {
            lines.push(Line::from(Span::styled(
                format!("    {chip_line}"),
                Style::default().fg(theme::TEXT_DIM()),
            )));
        }
    }
    let exits = if view.exits.is_empty() {
        "none".to_string()
    } else {
        view.exits
            .iter()
            .map(|(_, n)| n.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    lines.extend(side_kv_wrap("exits", &exits, theme::AMBER_DIM(), width));
    // Directly under the exits, because it answers the question the exits
    // raise: they say what is available, this says which one to take. A zone
    // boundary is a jump in the coordinate field rather than a direction, so
    // no picture of the world can carry this - only a named exit can.
    if let Some(heading) = heading {
        let (text, color) = match heading {
            Heading::Toward(name, route) => (
                format!(
                    "{} {name} · {} room{} · take {}",
                    route.next.compass_glyph(),
                    route.rooms,
                    if route.rooms == 1 { "" } else { "s" },
                    route.next.label()
                ),
                theme::SUCCESS(),
            ),
            Heading::Arrived(name) => (format!("\u{2691}{name} · you're here"), theme::SUCCESS()),
            Heading::Unreachable(name) => (
                format!("{name} · no way there over ground you know"),
                theme::ERROR(),
            ),
        };
        lines.extend(side_kv_wrap("compass", &text, color, width));
    }
    // A merchant standing here: called out on its own line, not buried in "Of
    // note", so a shop room can't be walked past without noticing it.
    if let Some(shop) = &view.shop {
        lines.extend(side_kv_wrap(
            "shop",
            &format!("{} ({})", shop.shop_name, shop.npc_name),
            theme::SUCCESS(),
            width,
        ));
    }
    if !view.features.is_empty() {
        lines.push(section("Of note"));
        for feat in &view.features {
            // Actionable things get a diamond marker so they pop like loot.
            let label = if is_actionable_feature(&feat.kind) {
                format!("◆ {}", feat.name)
            } else {
                format!("· {}", feat.name)
            };
            lines.extend(side_text_wrap(
                &label,
                interactable_color(&feat.kind),
                width,
            ));
        }
    }
    if !view.mobs.is_empty() {
        lines.push(section("Foes"));
        for mob in &view.mobs {
            let mut weight = if mob.boss {
                Modifier::BOLD
            } else {
                Modifier::empty()
            };
            // The foe you're locked onto gets a » marker so a click's effect is
            // visible; a boss keeps its ‡. The target is always bold.
            let marker = if mob.targeted {
                weight |= Modifier::BOLD;
                "\u{00bb} " // »
            } else if mob.boss {
                "\u{2021} " // ‡
            } else {
                "  "
            };
            let name_style = Style::default()
                .fg(rarity_color(&mob.rank))
                .add_modifier(weight);
            // The full name, wrapped - "a scrawny …" told nobody what they
            // were fighting. First line carries the marker (and the click
            // target); continuations hang indented under it.
            let wrap_w = width.saturating_sub(4).max(6);
            let wrapped = wrap_log_text(&format!("Lv{} {}", mob.level, mob.name), wrap_w);
            foe_hits.push((lines.len(), mob.id));
            for (i, part) in wrapped.into_iter().enumerate() {
                let prefix = if i == 0 { marker } else { "    " };
                lines.push(Line::from(Span::styled(
                    format!("{prefix}{part}"),
                    name_style,
                )));
            }
            // The health line: a wider meter plus real numbers, with the one
            // status that changes what to do next (a stunned foe is free hits).
            let mut meter_spans = vec![
                Span::raw("    "),
                Span::styled(
                    format!(
                        "{} {}/{}",
                        meter(mob.hp, mob.max_hp, 10),
                        mob.hp,
                        mob.max_hp
                    ),
                    Style::default().fg(hp_color(mob.hp, mob.max_hp)),
                ),
            ];
            if mob.dot_stacks > 0 {
                meter_spans.push(Span::styled(
                    format!(" bleed x{}", mob.dot_stacks),
                    Style::default().fg(theme::SUCCESS()),
                ));
            }
            if mob.stunned {
                meter_spans.push(Span::styled(
                    " stunned".to_string(),
                    Style::default().fg(theme::AMBER_GLOW()),
                ));
            }
            lines.push(Line::from(meter_spans));
        }
    }
    if !view.occupants.is_empty() {
        lines.push(section(if view.pvp {
            "Adventurers here (pvp ground)"
        } else {
            "Adventurers here"
        }));
        for occ in &view.occupants {
            let name = usernames
                .get(&occ.user_id)
                .cloned()
                .unwrap_or_else(|| "adventurer".to_string());
            let labelled = format!("Lv{:<2} {name}", occ.level);
            let following = view.following == Some(occ.user_id);
            let (status, color) = if !occ.alive {
                ("fallen", theme::ERROR())
            } else if following {
                ("follow", theme::MENTION())
            } else if occ.targeted {
                ("duel", theme::ERROR())
            } else if occ.in_combat {
                ("fight", theme::AMBER())
            } else if occ.attackable {
                ("hostile", theme::ERROR())
            } else {
                ("", theme::SUCCESS())
            };
            // Status (fallen/duel/fight/hostile) takes priority when there's
            // room for only one; otherwise the class abbreviation rides
            // alongside it so a foe's kit is visible before you ever engage.
            let abbrev = class_abbrev(&occ.class_key);
            let tag = match (status.is_empty(), abbrev.is_empty()) {
                (true, true) => String::new(),
                (true, false) => abbrev.to_string(),
                (false, true) => status.to_string(),
                (false, false) => format!("{status}\u{00b7}{abbrev}"),
            };
            let tag_w = if tag.is_empty() {
                0
            } else {
                1 + UnicodeWidthStr::width(tag.as_str())
            };
            let name_w = width.saturating_sub(13 + tag_w).clamp(6, 16);
            let marker = if occ.targeted { "\u{00bb} " } else { "  " };
            if occ.attackable {
                player_hits.push((lines.len(), occ.user_id));
            }
            lines.push(roster_row(
                marker,
                &labelled,
                occ.hp,
                occ.max_hp,
                Style::default().fg(color),
                name_w,
                &tag,
            ));
        }
    }
    if !view.wildlife.is_empty() {
        lines.push(section("Wildlife"));
        for w in &view.wildlife {
            // Mythical creatures (Genesys) always stand out, whatever else
            // they are - a boon or a huntable can still be a wonder to look at.
            let (marker, color) = if w.mythical {
                ("✵ ", theme::BOT())
            } else {
                match w.kind.as_str() {
                    "boon" => ("✦ ", theme::BADGE_GOLD()),
                    "huntable" => ("» ", theme::AMBER()),
                    _ => ("~ ", theme::TEXT_DIM()),
                }
            };
            let mut detail = if !w.perk.is_empty() {
                format!(", a boon ({})", w.perk)
            } else if w.kind == "huntable" {
                ", game (attack to hunt)".to_string()
            } else {
                String::new()
            };
            // The tutorial sentence is worth its two wrapped lines exactly
            // once: until the first feed. After that the player knows the
            // mechanic, and the live count is both shorter and more use than
            // being taught again every time they stand here.
            match (w.adoptable, w.adopt_streak) {
                (true, Some((fed, need))) => {
                    detail.push_str(&format!(" (~ {fed}/{need} days)"));
                }
                (true, None) => {
                    detail.push_str(", feed it daily (~) and it may take to you as a stray");
                }
                (false, _) => {}
            }
            lines.extend(side_text_wrap(
                &format!("{marker}{}{detail}", w.name),
                color,
                width,
            ));
        }
    }
    if !view.nodes.is_empty() {
        lines.push(section("Resources"));
        for n in &view.nodes {
            let (marker, color) = if n.gatherable {
                ("◆ ", theme::AMBER())
            } else {
                ("· ", theme::TEXT_DIM())
            };
            let detail = if n.gatherable {
                format!(", {} (press y)", n.skill.to_lowercase())
            } else if !n.reason.is_empty() {
                format!(", {}", n.reason)
            } else {
                String::new()
            };
            lines.extend(side_text_wrap(
                &format!("{marker}{}{detail}", n.name),
                color,
                width,
            ));
        }
    }
    // Tameable wild beasts of Broceliande: what roams here and whether you can
    // take it (the Animal Taming trade). Opened with `q`.
    if let Some(taming) = &view.taming
        && !taming.entries.is_empty()
    {
        lines.push(section("Wild beasts"));
        for e in &taming.entries {
            let (color, tail) = if e.reason.is_empty() {
                (theme::SUCCESS(), format!(", {}% to tame (press q)", e.odds))
            } else {
                (theme::TEXT_DIM(), format!(", {}", e.reason))
            };
            lines.extend(side_text_wrap(
                &format!("\u{1F43E} {}{tail}", e.name),
                color,
                width,
            ));
        }
    }
    RoomPanel {
        body: lines,
        footer: footer_hints(view, width),
        foe_hits,
        player_hits,
    }
}

/// The side panel while a fight is on, in the field layout: the room summary
/// gives way to a battle frame - your vitals, the locked foe's full name,
/// nature, and wide meter, your battle effects and companion, your ability
/// roster with live costs and readiness, and the room's other foes for
/// switching targets. The classic layout keeps the room summary, since its
/// main column already carries `battle_context`. Returns each clickable row's
/// line index with its action (switch the lock, cast an ability).
fn battle_side_panel(
    view: &PlayerView,
    usernames: &UsernameLookup<'_>,
    width: usize,
) -> (Vec<Line<'static>>, Vec<(usize, ClickAction)>) {
    let mut hits: Vec<(usize, ClickAction)> = Vec::new();
    let mut lines = vitals(view, VitalStyle::Meters(width));
    lines.push(Line::raw(""));
    lines.push(section("Battle"));
    let wrap_w = width.saturating_sub(4).max(6);
    // The panel draws without terminal wrapping (each row must stay one line
    // so clicks map to rows), so every line here is pre-wrapped or sized to
    // fit: meters shrink to leave room for their numbers, prose goes through
    // `side_text_wrap`, ability detail truncates.
    let hp_meter_line = |label: &'static str, hp: i32, max_hp: i32| {
        panel_meter_line(label, hp, max_hp, hp_color(hp, max_hp), width)
    };
    if let Some(mob) = view.mobs.iter().find(|m| m.targeted) {
        let name_style = Style::default()
            .fg(rarity_color(&mob.rank))
            .add_modifier(Modifier::BOLD);
        let marker = if mob.boss { "\u{2021} " } else { "\u{00bb} " };
        hits.push((lines.len(), ClickAction::AttackMob(mob.id)));
        for (i, part) in wrap_log_text(&format!("Lv{} {}", mob.level, mob.name), wrap_w)
            .into_iter()
            .enumerate()
        {
            let prefix = if i == 0 { marker } else { "    " };
            lines.push(Line::from(Span::styled(
                format!("{prefix}{part}"),
                name_style,
            )));
        }
        lines.push(hp_meter_line("  HP ", mob.hp, mob.max_hp));
        let mut traits: Vec<String> = vec![mob.rank.clone()];
        traits.push(format!("strikes with {}", mob.school));
        if let Some(weak) = mob.weak {
            traits.push(format!("weak to {weak}"));
        }
        if let Some(resist) = mob.resist {
            traits.push(format!("shrugs off {resist}"));
        }
        lines.extend(side_text_wrap(
            &traits.join(" \u{00b7} "),
            theme::TEXT_DIM(),
            width,
        ));
        let mut afflicted: Vec<String> = Vec::new();
        if mob.dot_stacks > 0 {
            afflicted.push(format!("bleeding x{}", mob.dot_stacks));
        }
        if mob.stunned {
            afflicted.push("stunned".to_string());
        }
        if !afflicted.is_empty() {
            lines.extend(side_text_wrap(
                &format!("afflicted: {}", afflicted.join(" \u{00b7} ")),
                theme::SUCCESS(),
                width,
            ));
        }
    } else if let Some(occ) = view.occupants.iter().find(|o| o.targeted) {
        let name = usernames
            .get(&occ.user_id)
            .cloned()
            .unwrap_or_else(|| "your rival".to_string());
        let name_style = Style::default()
            .fg(theme::ERROR())
            .add_modifier(Modifier::BOLD);
        for (i, part) in wrap_log_text(&format!("Lv{} {name}", occ.level), wrap_w)
            .into_iter()
            .enumerate()
        {
            let prefix = if i == 0 { "\u{00bb} " } else { "    " };
            lines.push(Line::from(Span::styled(
                format!("{prefix}{part}"),
                name_style,
            )));
        }
        lines.push(hp_meter_line("  HP ", occ.hp, occ.max_hp));
        lines.push(Line::from(Span::styled(
            "  duel",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    // Your own battle state, when there is any to show.
    let mut effects: Vec<String> = Vec::new();
    if view.shield > 0 {
        effects.push(format!("shield {}", view.shield));
    }
    if view.empower > 0 {
        effects.push(format!("empowered +{}", view.empower));
    }
    if let Some(coat) = &view.coat {
        effects.push(format!("{} coat x{}", coat.school, coat.charges));
    }
    if view.stunned {
        effects.push("stunned".to_string());
    }
    if !effects.is_empty() {
        lines.extend(side_text_wrap(
            &format!("you: {}", effects.join(" \u{00b7} ")),
            theme::AMBER_GLOW(),
            width,
        ));
    }
    // Your companion fighting alongside.
    if let Some(pet) = &view.pet {
        let (text, color) = if pet.downed {
            (format!("{} {} downed", pet.glyph, pet.name), theme::ERROR())
        } else {
            (
                format!("{} {} {}/{}", pet.glyph, pet.name, pet.hp, pet.max_hp),
                hp_color(pet.hp, pet.max_hp),
            )
        };
        lines.extend(side_text_wrap(&text, color, width));
        // Its unlocked auto-skills, so what fires by itself is no mystery.
        if !pet.downed && !pet.skills.is_empty() {
            let names = pet
                .skills
                .iter()
                .map(|(n, _)| n.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            lines.extend(side_text_wrap(
                &format!("    auto: {names}"),
                theme::AMBER_DIM(),
                width,
            ));
        }
    }
    // The ability roster, with live costs and readiness - what the bottom
    // action bar has no room to say. Each row casts on click, like its key.
    if !view.abilities.is_empty() {
        lines.push(Line::raw(""));
        lines.push(section("Abilities"));
        for a in &view.abilities {
            // Slot 10 is cast with `0`, matching the keybind; show that digit.
            let key = if a.slot == 10 { 0 } else { a.slot };
            let (key_style, name_color) = if a.ready {
                (theme::punch_through(theme::AMBER()), theme::TEXT_BRIGHT())
            } else {
                (
                    theme::punch_through(theme::BORDER_DIM()),
                    theme::TEXT_FAINT(),
                )
            };
            hits.push((lines.len(), ClickAction::Ability(a.slot)));
            let key_label = format!(" {key} ");
            let name = format!(" {}", a.name);
            let used =
                UnicodeWidthStr::width(key_label.as_str()) + UnicodeWidthStr::width(name.as_str());
            let detail = truncate_chars(
                &format!("  {}c {}", a.cost, a.effect),
                width.saturating_sub(used),
            );
            lines.push(Line::from(vec![
                Span::styled(key_label, key_style),
                Span::styled(
                    name,
                    Style::default().fg(name_color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(detail, Style::default().fg(theme::TEXT_DIM())),
            ]));
        }
    }
    // The room's other foes, so a click can switch the lock mid-fight.
    let others: Vec<&MobView> = view.mobs.iter().filter(|m| !m.targeted).collect();
    if !others.is_empty() {
        lines.push(Line::raw(""));
        lines.push(section("Also here"));
        for mob in others {
            let marker = if mob.boss { "\u{2021} " } else { "  " };
            let name_style = Style::default().fg(rarity_color(&mob.rank));
            hits.push((lines.len(), ClickAction::AttackMob(mob.id)));
            for (i, part) in wrap_log_text(&format!("Lv{} {}", mob.level, mob.name), wrap_w)
                .into_iter()
                .enumerate()
            {
                let prefix = if i == 0 { marker } else { "    " };
                lines.push(Line::from(Span::styled(
                    format!("{prefix}{part}"),
                    name_style,
                )));
            }
            lines.push(Line::from(Span::styled(
                format!(
                    "    {} {}/{}",
                    meter(mob.hp, mob.max_hp, 10),
                    mob.hp,
                    mob.max_hp
                ),
                Style::default().fg(hp_color(mob.hp, mob.max_hp)),
            )));
        }
        lines.push(hint("click", "switch target"));
    }
    lines.push(Line::raw(""));
    lines.push(hint("space/x", "strike  z flee"));
    lines.push(hint("Q", "quaff a potion"));
    lines.push(hint("C", "coat your weapon"));
    (lines, hits)
}

/// The overhead minimap section: a small map of the explored neighbourhood,
/// painted in the bottom corner of the Room panel.
fn minimap_lines(map: &MiniMap) -> Vec<Line<'static>> {
    if map.grid.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![section("Map")];
    for row in &map.grid {
        let mut spans = vec![Span::raw("  ")];
        spans.extend(row.iter().map(|cell| map_cell_span(*cell)));
        lines.push(Line::from(spans));
    }
    // Vertical exits can't sit on a flat map; note them in words instead.
    let mut stairs = Vec::new();
    if map.up {
        stairs.push("up");
    }
    if map.down {
        stairs.push("down");
    }
    let stairs_text = if stairs.is_empty() {
        String::new()
    } else {
        format!("stairs: {}", stairs.join(", "))
    };
    lines.push(Line::from(Span::styled(
        format!("  {stairs_text:<18}"),
        Style::default().fg(theme::TEXT_DIM()),
    )));
    lines.push(Line::from(Span::styled(
        "  @=you *=last o=seen .=new",
        Style::default().fg(theme::TEXT_FAINT()),
    )));
    lines
}

/// One char-cell of the minimap, styled by what it represents.
fn map_cell_span(cell: MapCell) -> Span<'static> {
    let (glyph, color) = match cell {
        MapCell::Empty => (' ', theme::TEXT_FAINT()),
        MapCell::Current => ('@', theme::AMBER_GLOW()),
        MapCell::Previous => ('*', theme::AMBER()),
        MapCell::Visited => ('o', theme::AMBER_DIM()),
        MapCell::Frontier => ('.', theme::TEXT_FAINT()),
        MapCell::ConnH => ('-', theme::BORDER()),
        MapCell::ConnV => ('|', theme::BORDER()),
        MapCell::TrailH => ('-', theme::AMBER_GLOW()),
        MapCell::TrailV => ('|', theme::AMBER_GLOW()),
    };
    let mut style = Style::default().fg(color);
    if matches!(cell, MapCell::Current | MapCell::Previous) {
        style = style.add_modifier(Modifier::BOLD);
    }
    Span::styled(glyph.to_string(), style)
}

// ---- The character sheet -------------------------------------------------

/// Full-width character dashboard (the `c` panel when the terminal is roomy).
/// A class portrait and vitals bars on the left, ability scores as dot ratings
/// in the middle, and combat/derived stats, trait, titles, and XP on the right.
/// `[`/`]` scroll all three columns together: `scroll` is the requested
/// offset in lines, and the clamped one comes back for the caller to keep.
/// It stops once the tallest column's last row reaches the floor; a shorter
/// column stops at its own end rather than scrolling into blank.
fn draw_character_sheet(frame: &mut Frame, area: Rect, view: &PlayerView, scroll: usize) -> usize {
    let accent = class_accent(Class::from_key(&view.class_key));
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()))
        .title(Span::styled(
            " Character ",
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let cols = Layout::horizontal([
        Constraint::Length(20),
        Constraint::Min(22),
        Constraint::Min(20),
    ])
    .split(inner);

    let columns = [
        (sheet_identity(view, accent), cols[0]),
        (sheet_attributes(view, accent), cols[1]),
        (sheet_derived(view, accent), cols[2]),
    ];
    let max_offsets = columns.each_ref().map(|(lines, col)| {
        scroll_offset(
            usize::MAX,
            lines,
            None,
            col.width as usize,
            col.height as usize,
        )
    });
    let off = scroll.min(max_offsets.iter().copied().max().unwrap_or(0));
    for ((mut lines, col), max_off) in columns.into_iter().zip(max_offsets) {
        let shown = lines.split_off(off.min(max_off));
        frame.render_widget(Paragraph::new(shown).wrap(Wrap { trim: false }), col);
    }
    off
}

/// Left column: portrait, identity headline, and vitals as filled meters.
fn sheet_identity(view: &PlayerView, accent: Color) -> Vec<Line<'static>> {
    let mut lines = composed_portrait(&view.class_key, &view.appearance_idx, accent);
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        format!("Lv {} {}", view.level, view.class_name),
        Style::default().fg(accent).add_modifier(Modifier::BOLD),
    )));
    if let Some((name, role)) = &view.archetype {
        lines.push(Line::from(Span::styled(
            format!("⟡ {name} · {role}"),
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        )));
    }
    if let Some(milestone) = super::classes::current_milestone(view.level) {
        lines.push(Line::from(Span::styled(
            format!("✦ {milestone}"),
            Style::default().fg(theme::AMBER_DIM()),
        )));
    }
    if let Some(title) = view.active_title.and_then(|i| view.titles.get(i)) {
        lines.push(Line::from(Span::styled(
            title.clone(),
            Style::default().fg(theme::BADGE_GOLD()),
        )));
    }
    lines.push(Line::raw(""));
    lines.push(bar_line(
        "HP",
        view.hp,
        view.max_hp,
        hp_color(view.hp, view.max_hp),
    ));
    lines.push(meter_caption(view.hp, view.max_hp));
    let res = short_res(&view.resource_name);
    lines.push(bar_line(
        &res,
        view.resource,
        view.max_resource,
        theme::MENTION(),
    ));
    lines.push(meter_caption(view.resource, view.max_resource));
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled("gold ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled(
            view.gold.to_string(),
            Style::default().fg(theme::BADGE_GOLD()),
        ),
    ]));
    if view.banked_gold > 0 {
        lines.push(Line::from(vec![
            Span::styled("bank ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                view.banked_gold.to_string(),
                Style::default().fg(theme::TEXT_BRIGHT()),
            ),
        ]));
    }
    lines
}

/// Middle column: the six ability scores as dot ratings, the class's primary
/// score highlighted, then the passive trait.
/// The six ability scores as rated rows: a star-rated header, then each
/// score's value coloured by tier plus its own 5-star rating; the `primary`
/// score glows in the class `accent`. Shared by the character sheet and the
/// creation screen so the rolled fate reads the same as the finished hero.
fn attribute_lines(view: &PlayerView, primary: &str, accent: Color) -> Vec<Line<'static>> {
    let rows = view.scores.rows();
    let avg = if rows.is_empty() {
        0
    } else {
        rows.iter().map(|(_, v, _)| *v).sum::<i32>() / rows.len() as i32
    };
    let mut lines = vec![section_stars("Attributes", avg, 18, accent)];
    for (label, value, modifier) in rows {
        let sign = if modifier >= 0 { "+" } else { "" };
        let is_primary = label == primary;
        let label_color = if is_primary {
            accent
        } else {
            theme::TEXT_DIM()
        };
        let weight = if is_primary {
            Modifier::BOLD
        } else {
            Modifier::empty()
        };
        // The value coloured by tier (weak/faint → strong/green); the primary
        // score's stars glow in the class accent, the rest in their tier colour.
        let star_color = if is_primary {
            accent
        } else {
            tier_color(value, 18)
        };
        let mut spans = vec![
            Span::styled(
                format!("  {label} "),
                Style::default().fg(label_color).add_modifier(weight),
            ),
            Span::styled(
                format!("{value:>2}({sign}{modifier}) "),
                Style::default().fg(tier_color(value, 18)),
            ),
        ];
        spans.extend(star_rating(value, 18, star_color));
        lines.push(Line::from(spans));
    }
    lines
}

fn sheet_attributes(view: &PlayerView, accent: Color) -> Vec<Line<'static>> {
    let mut lines = attribute_lines(
        view,
        primary_label(Class::from_key(&view.class_key)),
        accent,
    );
    lines.push(Line::raw(""));
    lines.push(section("Trait"));
    lines.push(Line::from(Span::styled(
        format!("  {}", view.trait_name),
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    )));
    lines.extend(wrap(&view.trait_desc, 24));
    lines.push(Line::raw(""));
    lines.extend(skills_block(view));
    lines
}

/// Earned titles shown inline on the character sheet before the list is
/// summarised. The full list is browsable in the Titles panel (`k`); the sheet
/// is a fixed-height column with no scroll of its own, so an unbounded list
/// here used to push the XP meter clean off the bottom.
const SHEET_TITLES_SHOWN: usize = 4;

/// Right column: combat numbers, revives, the XP meter, then earned titles.
/// Experience comes first on purpose: it is the number you always want, and
/// titles are the section that grows without limit.
fn sheet_derived(view: &PlayerView, accent: Color) -> Vec<Line<'static>> {
    // Combat rated by level; attack reads as offence (green), armour as
    // defence (blue), split for clarity.
    let mut lines = vec![section_stars("Combat", view.level, 50, accent)];
    lines.push(stat_colored(
        "attack",
        format!("+{}", view.attack),
        theme::SUCCESS(),
    ));
    lines.push(stat_colored(
        "swing",
        format!("+{}", view.swing),
        theme::SUCCESS(),
    ));
    lines.push(stat_colored(
        "spell",
        format!("+{}", view.spell_power),
        theme::SUCCESS(),
    ));
    lines.push(stat_colored(
        "armor",
        view.armor.to_string(),
        theme::MENTION(),
    ));
    if view.resurrection_cap > 0 {
        lines.push(stat_colored(
            "revives",
            format!("{}/{}", view.resurrections_left, view.resurrection_cap),
            theme::AMBER(),
        ));
    }
    lines.push(Line::raw(""));
    lines.push(section("Experience"));
    if view.xp_for_next > 0 {
        lines.push(Line::from(Span::styled(
            format!(
                "  {}",
                meter(view.xp_into_level as i32, view.xp_for_next as i32, 14)
            ),
            Style::default().fg(accent),
        )));
        lines.push(Line::from(Span::styled(
            format!("  {}/{} to next", view.xp_into_level, view.xp_for_next),
            Style::default().fg(theme::TEXT_DIM()),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "  max level reached",
            Style::default().fg(theme::BADGE_GOLD()),
        )));
    }
    lines.push(Line::raw(""));
    lines.push(section("Titles"));
    if view.titles.is_empty() {
        lines.push(Line::from(Span::styled(
            "  none yet",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    for title in view.titles.iter().take(SHEET_TITLES_SHOWN) {
        lines.push(Line::from(Span::styled(
            format!("  {title}"),
            Style::default().fg(theme::BADGE_GOLD()),
        )));
    }
    let rest = view.titles.len().saturating_sub(SHEET_TITLES_SHOWN);
    if rest > 0 {
        lines.push(Line::from(Span::styled(
            format!("  +{rest} more (k)"),
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    lines.push(Line::raw(""));
    lines.push(hint("c", "close  v abilities  t bag"));
    lines
}

/// The gathering-trades block: each skill's level and a compact progress
/// readout. Shown on both the full character sheet and the narrow panel; the `y`
/// hint teaches how to work a node.
fn skills_block(view: &PlayerView) -> Vec<Line<'static>> {
    let avg = if view.skills.is_empty() {
        0
    } else {
        view.skills.iter().map(|s| s.level).sum::<i32>() / view.skills.len() as i32
    };
    let mut lines = vec![section_stars("Trades", avg, 50, theme::AMBER())];
    if view.skills.is_empty() {
        lines.push(Line::from(Span::styled(
            "  untrained",
            Style::default().fg(theme::TEXT_DIM()),
        )));
        return lines;
    }
    for s in &view.skills {
        let progress = if s.xp_next > 0 {
            format!("{}/{}", s.xp_into, s.xp_next)
        } else {
            "max".to_string()
        };
        lines.push(Line::from(vec![
            Span::styled(format!("  {} ", s.name), Style::default().fg(theme::TEXT())),
            Span::styled(
                format!("L{}", s.level),
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {progress}"),
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ]));
    }
    lines.push(hint("y", "gather a resource node"));
    lines
}

// ---- Meters, stars, portraits, and the class palette ---------------------

/// A labelled filled meter line, e.g. `HP   ███████░░░`.
fn bar_line(label: &str, cur: i32, max: i32, color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label:<4} "),
            Style::default().fg(theme::TEXT_DIM()),
        ),
        Span::styled(meter(cur, max, 10), Style::default().fg(color)),
    ])
}

/// The dimmed `cur/max` caption printed under a meter.
fn meter_caption(cur: i32, max: i32) -> Line<'static> {
    Line::from(Span::styled(
        format!("     {cur}/{max}"),
        Style::default().fg(theme::TEXT_DIM()),
    ))
}

/// A 5-star rating: filled stars in `color`, the remainder dim. `value` is
/// scored out of `max`.
fn star_rating(value: i32, max: i32, color: Color) -> Vec<Span<'static>> {
    const OF: i32 = 5;
    let filled = if max <= 0 {
        0
    } else {
        ((value.clamp(0, max) * OF + max / 2) / max).clamp(0, OF)
    } as usize;
    vec![
        Span::styled("★".repeat(filled), Style::default().fg(color)),
        Span::styled(
            "☆".repeat(OF as usize - filled),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
    ]
}

/// Colour a numeric attribute by how strong it is out of `max`: faint for a
/// weak stat, amber for middling, green for strong — so strengths and
/// weaknesses pop at a glance.
fn tier_color(value: i32, max: i32) -> Color {
    if max <= 0 {
        return theme::TEXT_DIM();
    }
    let pct = value.clamp(0, max) * 100 / max;
    if pct >= 66 {
        theme::SUCCESS()
    } else if pct >= 33 {
        theme::AMBER()
    } else {
        theme::TEXT_DIM()
    }
}

/// A section header carrying a 5-star rating, e.g. ` - Attributes ★★★☆☆`.
fn section_stars(title: &str, value: i32, max: i32, accent: Color) -> Line<'static> {
    let mut spans = vec![
        Span::styled(" - ", Style::default().fg(theme::BORDER())),
        Span::styled(
            format!("{title} "),
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ),
    ];
    spans.extend(star_rating(value, max, accent));
    Line::from(spans)
}

/// A filled progress meter (`█████░░░`) of the given cell width.
fn meter(cur: i32, max: i32, width: usize) -> String {
    let width = width.max(1);
    let filled = if max <= 0 {
        0
    } else {
        ((cur.max(0) as i64 * width as i64) / max as i64) as usize
    }
    .min(width);
    (0..width)
        .map(|i| if i < filled { '█' } else { '░' })
        .collect()
}

/// One meter row in the field layout's side panel: a dim label in a fixed
/// gutter, then the bar and its numbers in the row's own color. The bar shrinks
/// to leave room for the numbers, since the panel draws without terminal
/// wrapping and every row must stay a single line for click mapping. Both sides
/// of a fight render through here, so your bar and the foe's are the same
/// object at the same width.
fn panel_meter_line(label: &str, cur: i32, max: i32, color: Color, width: usize) -> Line<'static> {
    let nums = format!("{cur}/{max}");
    let meter_w = width
        .saturating_sub(UnicodeWidthStr::width(label) + UnicodeWidthStr::width(nums.as_str()) + 1)
        .clamp(6, 22);
    Line::from(vec![
        Span::styled(label.to_string(), Style::default().fg(theme::TEXT_DIM())),
        Span::styled(
            format!("{} {nums}", meter(cur, max, meter_w)),
            Style::default().fg(color),
        ),
    ])
}

/// Truncate (with an ellipsis) or right-pad `name` to exactly `w` display
/// columns, so roster rows line up into clean columns regardless of name width.
fn fit(name: &str, w: usize) -> String {
    let width = UnicodeWidthStr::width(name);
    if width <= w {
        return format!("{name}{}", " ".repeat(w - width));
    }
    let mut out = String::new();
    let mut used = 0usize;
    for ch in name.chars() {
        let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + cw > w.saturating_sub(1) {
            break;
        }
        out.push(ch);
        used += cw;
    }
    out.push('…');
    used += 1;
    if used < w {
        out.push_str(&" ".repeat(w - used));
    }
    out
}

/// One aligned roster row: a marker, the name padded to `name_w`, a small HP
/// meter tinted by remaining health, and an optional trailing tag. Shared by
/// the party (Follow) panel and the room's Foes / Adventurers tables.
fn roster_row(
    marker: &str,
    name: &str,
    hp: i32,
    max_hp: i32,
    name_style: Style,
    name_w: usize,
    tag: &str,
) -> Line<'static> {
    let mut spans = vec![
        Span::styled(marker.to_string(), name_style),
        Span::styled(fit(name, name_w), name_style),
        Span::raw(" "),
        Span::styled(
            meter(hp, max_hp, 6),
            Style::default().fg(hp_color(hp, max_hp)),
        ),
    ];
    if !tag.is_empty() {
        spans.push(Span::styled(
            format!(" {tag}"),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    Line::from(spans)
}

/// The attribute row a class's key ability lands on, so the sheet and the
/// selection screen glow the score that actually feeds its attack bonus. Read
/// off the class itself: a new class cannot silently render with no attribute
/// highlighted. Empty for a player who has not picked a class yet.
fn primary_label(class: Option<Class>) -> &'static str {
    match class {
        Some(class) => class.primary_score().label(),
        None => "",
    }
}

/// A three-letter class abbreviation, for roster rows too narrow for the full
/// name (hand-picked, not a naive truncation - "Warrior"/"Warlock" would
/// otherwise collide on "WAR").
fn class_abbrev(class_key: &str) -> &'static str {
    match Class::from_key(class_key) {
        Some(Class::Warrior) => "WAR",
        Some(Class::Mage) => "MAG",
        Some(Class::Cleric) => "CLR",
        Some(Class::Rogue) => "ROG",
        Some(Class::Ranger) => "RNG",
        Some(Class::Druid) => "DRU",
        Some(Class::Necromancer) => "NEC",
        Some(Class::Bard) => "BRD",
        Some(Class::Monk) => "MNK",
        Some(Class::Paladin) => "PAL",
        Some(Class::Warlock) => "WLK",
        Some(Class::Berserker) => "BRS",
        Some(Class::Beastlord) => "BST",
        Some(Class::Skald) => "SKD",
        Some(Class::Runemaster) => "RUN",
        Some(Class::Valewalker) => "VLW",
        Some(Class::Spiritmaster) => "SPM",
        None => "",
    }
}

/// The colour a class is drawn in: its portrait tint, its key attribute, the
/// stars on its rated rows. Every class names one, so no calling reads as a
/// colourless "generic adventurer". Unclassed players get plain bright text.
fn class_accent(class: Option<Class>) -> Color {
    let Some(class) = class else {
        return theme::TEXT_BRIGHT();
    };
    match class {
        Class::Warrior => theme::AMBER(),
        Class::Mage => theme::MENTION(),
        Class::Cleric => theme::BADGE_GOLD(),
        Class::Rogue => theme::ERROR(),
        Class::Ranger => theme::SUCCESS(),
        Class::Druid => theme::SUCCESS(),
        Class::Necromancer => theme::MENTION(),
        Class::Bard => theme::AMBER_GLOW(),
        Class::Monk => theme::BADGE_GOLD(),
        Class::Paladin => theme::BADGE_GOLD(),
        Class::Warlock => theme::ERROR(),
        Class::Berserker => theme::AMBER(),
        Class::Beastlord => theme::SUCCESS(),
        Class::Skald => theme::AMBER_GLOW(),
        Class::Runemaster => theme::MENTION(),
        Class::Valewalker => theme::SUCCESS(),
        Class::Spiritmaster => theme::MENTION(),
    }
}

/// The mark a class stands under: a single glyph and its name, shown beneath
/// the portrait bust. Every calling names its own, and no two share a glyph, so
/// a bust reads as that class at a glance. Unclassed players are Adventurers.
fn class_emblem(class: Option<Class>) -> &'static str {
    let Some(class) = class else {
        return "Adventurer";
    };
    match class {
        Class::Warrior => "⚔ Warrior",
        Class::Mage => "✦ Mage",
        Class::Cleric => "✚ Cleric",
        Class::Rogue => "† Rogue",
        Class::Ranger => "➹ Ranger",
        Class::Druid => "☘ Druid",
        Class::Necromancer => "☠ Necromancer",
        Class::Bard => "♫ Bard",
        Class::Monk => "☯ Monk",
        Class::Paladin => "✠ Paladin",
        Class::Warlock => "☾ Warlock",
        Class::Berserker => "☄ Berserker",
        Class::Beastlord => "❦ Beastlord",
        Class::Skald => "♪ Skald",
        Class::Runemaster => "ᛟ Runemaster",
        Class::Valewalker => "⚑ Valewalker",
        Class::Spiritmaster => "✵ Spiritmaster",
    }
}

/// A hair-colour tint for the portrait fringe, from the Hair option index.
fn hair_tint(idx: u8) -> Color {
    match idx {
        3 | 9 => theme::TEXT_DIM(),   // silver-streaked / salt-and-pepper
        5 => theme::TEXT_BRIGHT(),    // raven-dark (near-black reads as bright ink)
        6 => theme::ERROR(),          // fire-red
        7 | 8 => theme::BADGE_GOLD(), // sun-bleached / ash-blond
        _ => theme::AMBER_DIM(),
    }
}

/// An eye-colour tint for the portrait, from the Eyes option index.
fn eye_tint(idx: u8) -> Color {
    match idx {
        1 | 3 | 9 => theme::AMBER(), // warm brown / amber / hazel
        2 | 10 => theme::MENTION(),  // pale blue / ice-pale
        7 => theme::SUCCESS(),       // glass-green
        6 => theme::TEXT_DIM(),      // storm-dark
        11 => theme::BADGE_GOLD(),   // gold-flecked
        _ => theme::TEXT_BRIGHT(),
    }
}

/// A composed ASCII portrait bust built from the player's own appearance choices
/// (build/hair/eyes/bearing) plus a class-flavoured headpiece, tinted with the
/// class accent and per-feature colours. Falls back cleanly when indices are
/// missing (old/absent selections). The class emblem is shown below the bust.
fn composed_portrait(class_key: &str, sel: &[u8], accent: Color) -> Vec<Line<'static>> {
    // Pad/clamp the selection to a full field set.
    let mut idx = [0u8; appearance::N_FIELDS];
    for (i, slot) in idx.iter_mut().enumerate() {
        *slot = sel.get(i).copied().unwrap_or(0);
    }
    let rows = appearance::portrait(class_key, &idx);
    // Row roles: 0 adornment (accent), 1 hair (hair tint), 3 eyes (eye tint),
    // the rest the neutral face frame.
    let hair = hair_tint(idx[1]);
    let eyes = eye_tint(idx[2]);
    let mut lines: Vec<Line<'static>> = rows
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let (color, weight) = match i {
                0 => (accent, Modifier::BOLD),
                1 => (hair, Modifier::empty()),
                3 => (eyes, Modifier::BOLD),
                _ => (theme::TEXT_BRIGHT(), Modifier::empty()),
            };
            Line::from(Span::styled(
                row.clone(),
                Style::default().fg(color).add_modifier(weight),
            ))
        })
        .collect();
    lines.push(Line::from(Span::styled(
        format!(" {}", class_emblem(Class::from_key(class_key))),
        Style::default().fg(accent).add_modifier(Modifier::BOLD),
    )));
    lines
}

// ---- The panel behind each key: pack, shop, craft, tame, atlas -----------

fn character_panel(view: &PlayerView) -> Vec<Line<'static>> {
    let mut lines = vitals(view, VitalStyle::Numbers);
    lines.push(Line::raw(""));
    lines.push(section("Combat"));
    lines.push(stat("attack", view.attack.to_string()));
    lines.push(stat("swing", view.swing.to_string()));
    lines.push(stat("spell", view.spell_power.to_string()));
    lines.push(stat("armor", view.armor.to_string()));
    lines.push(Line::raw(""));
    lines.extend(attribute_lines(
        view,
        primary_label(Class::from_key(&view.class_key)),
        class_accent(Class::from_key(&view.class_key)),
    ));
    if view.resurrection_cap > 0 {
        lines.push(stat(
            "revives",
            format!("{}/{}", view.resurrections_left, view.resurrection_cap),
        ));
    }
    lines.push(Line::raw(""));
    lines.push(section("Trait"));
    lines.push(Line::from(Span::styled(
        format!("  {}", view.trait_name),
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    )));
    lines.extend(wrap(&view.trait_desc, 30));
    if !view.bio.is_empty() {
        lines.push(Line::raw(""));
        lines.push(section("Bio"));
        for l in wrap(&view.bio, 30) {
            lines.push(l);
        }
        lines.push(hint("e", "edit appearance"));
    }
    if !view.titles.is_empty() {
        lines.push(Line::raw(""));
        lines.push(section("Titles"));
        for title in &view.titles {
            lines.push(Line::from(Span::styled(
                format!("  {title}"),
                Style::default().fg(theme::BADGE_GOLD()),
            )));
        }
    }
    lines.push(Line::raw(""));
    lines.push(section("Experience"));
    if view.xp_for_next > 0 {
        lines.push(Line::from(Span::styled(
            format!("  {}/{} to next", view.xp_into_level, view.xp_for_next),
            Style::default().fg(theme::TEXT()),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "  max level reached",
            Style::default().fg(theme::BADGE_GOLD()),
        )));
    }
    lines.push(Line::raw(""));
    lines.extend(skills_block(view));
    lines.push(Line::raw(""));
    lines.push(hint("c", "close  v abilities  t bag"));
    lines.push(hint("[ ]", "scroll"));
    lines
}

/// Examine panel: the lookable things in the current room.
fn examine_panel(view: &PlayerView, cursor: usize) -> (Vec<Line<'static>>, Option<usize>) {
    let mut lines = vec![section("Look at")];
    let mut sel_line = None;
    if view.features.is_empty() {
        lines.push(Line::from(Span::styled(
            "  nothing of note here",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    for (i, feat) in view.features.iter().enumerate() {
        let selected = i == cursor;
        if selected {
            sel_line = Some(lines.len());
        }
        let marker = if selected { ">" } else { " " };
        let tag = if feat.kind.is_empty() {
            String::new()
        } else {
            format!(" [{}]", feat.kind)
        };
        let actionable = is_actionable_feature(&feat.kind);
        let style = if selected {
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .patch(theme::selection_style())
                .add_modifier(Modifier::BOLD)
        } else if actionable {
            Style::default()
                .fg(interactable_color(&feat.kind))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(interactable_color(&feat.kind))
        };
        let bullet = if actionable { "◆" } else { marker };
        lines.push(Line::from(Span::styled(
            format!("{bullet} {}{}", feat.name, tag),
            style,
        )));
    }
    lines.push(Line::raw(""));
    lines.push(hint("w/s", "select  Enter look"));
    lines.push(hint("o", "close"));
    (lines, sel_line)
}

/// One compact line of the six ability scores with their modifiers.
fn abilities_panel(view: &PlayerView, cursor: usize) -> (Vec<Line<'static>>, Option<usize>) {
    let mut lines = vec![section("Abilities")];
    let mut sel_line = None;
    if view.abilities.is_empty() {
        lines.push(Line::from(Span::styled(
            "  none yet",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    for (i, a) in view.abilities.iter().enumerate() {
        let selected = i == cursor;
        if selected {
            sel_line = Some(lines.len());
        }
        let color = if a.ready {
            theme::TEXT_BRIGHT()
        } else {
            theme::TEXT_FAINT()
        };
        let marker = if selected { ">" } else { " " };
        lines.push(Line::from(vec![
            Span::styled(
                marker.to_string(),
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:>2} ", a.slot),
                theme::punch_through(if a.ready {
                    theme::AMBER()
                } else {
                    theme::BORDER_DIM()
                }),
            ),
            Span::styled(
                format!(" {}", a.name),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {}c {}", a.cost, a.effect),
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(hint("Enter", "cast selected  1-9 cast that slot"));
    lines.push(hint("0", "casts slot 10 while adventuring"));
    lines.push(hint("v", "close"));
    (lines, sel_line)
}

fn inventory_panel(
    rows: &[SectionRow],
    view: &PlayerView,
    cursor: usize,
) -> (Vec<Line<'static>>, Option<usize>) {
    let mut sel_line = None;
    let mut lines = vec![
        section("Inventory"),
        Line::from(Span::styled(
            format!("  {} gold", view.gold),
            Style::default().fg(theme::BADGE_GOLD()),
        )),
    ];
    if view.banked_gold > 0 {
        lines.push(Line::from(Span::styled(
            format!("  {} banked", view.banked_gold),
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    if view.inventory.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (empty)",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    for (i, row) in rows.iter().enumerate() {
        let selected = i == cursor;
        if selected {
            sel_line = Some(lines.len());
        }
        match row {
            SectionRow::Header {
                label,
                count,
                collapsed,
                ..
            } => lines.push(section_header_line(label, *count, *collapsed, selected)),
            SectionRow::Item { index } => {
                let Some(it) = view.inventory.get(*index) else {
                    continue;
                };
                let marker = if selected { ">" } else { " " };
                let tag = inventory_item_tag(it.equipped, it.slot.as_deref());
                let style = if selected {
                    Style::default()
                        .fg(theme::TEXT_BRIGHT())
                        .patch(theme::selection_style())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(rarity_color(&it.rarity))
                };
                lines.push(Line::from(vec![Span::styled(
                    format!("{marker} {}{}", it.name, tag),
                    style,
                )]));
                if !it.stats.is_empty() {
                    let mut stat_spans = vec![Span::styled(
                        format!("    {}", it.stats),
                        Style::default().fg(theme::TEXT_DIM()),
                    )];
                    if let Some(cmp) = compare_span(it.compare_pct) {
                        stat_spans.push(cmp);
                    }
                    lines.push(Line::from(stat_spans));
                }
                if let Some(cmp) = compare_line(&it.compare) {
                    lines.push(cmp);
                }
                if !it.desc.is_empty() {
                    lines.push(Line::from(Span::styled(
                        format!("    {}", it.desc),
                        Style::default().fg(theme::TEXT_FAINT()),
                    )));
                }
            }
        }
    }
    lines.push(Line::raw(""));
    lines.push(hint("w/s", "select  Enter equip/use/fold"));
    lines.push(hint("x", "sell one (at a shop)"));
    lines.push(hint("A/C/J", "sell all / commons / non-upgrades"));
    lines.push(hint("t", "close"));
    (lines, sel_line)
}

/// A coloured "vs worn" comparison line for a gear row: green for an upgrade,
/// red for a downgrade, amber for a mixed trade-off. None when there's nothing
/// to compare.
fn compare_line(compare: &str) -> Option<Line<'static>> {
    if compare.is_empty() {
        return None;
    }
    let up = compare == "new slot" || compare.contains('+');
    let down = compare.contains('-');
    let color = match (up, down) {
        (true, false) => theme::SUCCESS(),
        (false, true) => theme::ERROR(),
        _ => theme::AMBER(),
    };
    Some(Line::from(Span::styled(
        format!("    {compare}"),
        Style::default().fg(color),
    )))
}

/// A small coloured " ▲+18%" / " ▼-12%" tag comparing gear to what's worn: green
/// for an upgrade, red for worse, faint for a sidegrade. None renders nothing.
fn compare_span(compare_pct: Option<i32>) -> Option<Span<'static>> {
    let pct = compare_pct?;
    let (arrow, color) = if pct > 0 {
        ('\u{25B2}', theme::SUCCESS())
    } else if pct < 0 {
        ('\u{25BC}', theme::ERROR())
    } else {
        ('=', theme::TEXT_DIM())
    };
    Some(Span::styled(
        format!("  {arrow}{pct:+}%"),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    ))
}

fn inventory_item_tag(equipped: bool, slot: Option<&str>) -> String {
    if equipped {
        return slot
            .map(|slot| format!(" [worn {slot}]"))
            .unwrap_or_else(|| " [worn]".to_string());
    }
    slot.map(|slot| format!(" ({slot})")).unwrap_or_default()
}

fn shop_panel(
    rows: &[SectionRow],
    view: &PlayerView,
    cursor: usize,
) -> (Vec<Line<'static>>, Option<usize>) {
    let Some(shop) = &view.shop else {
        return (
            vec![Line::from(Span::styled(
                "No shop here.",
                Style::default().fg(theme::TEXT_DIM()),
            ))],
            None,
        );
    };
    let mut sel_line = None;
    let gold_line = if view.banked_gold > 0 {
        format!(
            "{} - your gold: {} (bank: {})",
            shop.npc_name, view.gold, view.banked_gold
        )
    } else {
        format!("{} - your gold: {}", shop.npc_name, view.gold)
    };
    let mut lines = vec![
        Line::from(Span::styled(
            shop.shop_name.clone(),
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            gold_line,
            Style::default().fg(theme::TEXT_DIM()),
        )),
        Line::raw(""),
    ];
    for (i, row) in rows.iter().enumerate() {
        let selected = i == cursor;
        if selected {
            sel_line = Some(lines.len());
        }
        match row {
            SectionRow::Header {
                label,
                count,
                collapsed,
                ..
            } => lines.push(section_header_line(label, *count, *collapsed, selected)),
            SectionRow::Item { index } => {
                let Some(e) = shop.entries.get(*index) else {
                    continue;
                };
                let marker = if selected { ">" } else { " " };
                let price_color = if e.affordable {
                    theme::BADGE_GOLD()
                } else {
                    theme::ERROR()
                };
                let name_style = if selected {
                    Style::default()
                        .fg(theme::TEXT_BRIGHT())
                        .patch(theme::selection_style())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(rarity_color(&e.rarity))
                };
                let mut spans = vec![Span::styled(format!("{marker} {}", e.name), name_style)];
                if !e.stats.is_empty() {
                    lines.push(Line::from(spans));
                    let mut stat_spans = vec![
                        Span::styled(
                            format!("    {}", e.stats),
                            Style::default().fg(theme::TEXT_DIM()),
                        ),
                        Span::styled(format!("  {}g", e.price), Style::default().fg(price_color)),
                    ];
                    if let Some(cmp) = compare_span(e.compare_pct) {
                        stat_spans.push(cmp);
                    }
                    lines.push(Line::from(stat_spans));
                    if let Some(cmp) = compare_line(&e.compare) {
                        lines.push(cmp);
                    }
                } else {
                    spans.push(Span::styled(
                        format!("  {}g", e.price),
                        Style::default().fg(price_color),
                    ));
                    lines.push(Line::from(spans));
                }
                if !e.desc.is_empty() {
                    lines.push(Line::from(Span::styled(
                        format!("    {}", e.desc),
                        Style::default().fg(theme::TEXT_FAINT()),
                    )));
                }
            }
        }
    }
    lines.push(Line::raw(""));
    lines.push(hint("w/s", "select  Enter buy/fold"));
    lines.push(hint("b", "leave shop"));
    (lines, sel_line)
}

/// A collapsible category header line: "▾ Weapons (3)" / "▸ Weapons (3)".
fn section_header_line(
    label: &str,
    count: usize,
    collapsed: bool,
    selected: bool,
) -> Line<'static> {
    let arrow = if collapsed { "▸" } else { "▾" };
    let style = if selected {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .patch(theme::selection_style())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD)
    };
    Line::from(Span::styled(format!("{arrow} {label} ({count})"), style))
}

fn crafting_panel(
    rows: &[SectionRow],
    view: &PlayerView,
    cursor: usize,
) -> (Vec<Line<'static>>, Option<usize>) {
    let Some(craft) = &view.crafting else {
        return (
            vec![Line::from(Span::styled(
                "No crafting station here.",
                Style::default().fg(theme::TEXT_DIM()),
            ))],
            None,
        );
    };
    let mut sel_line = None;
    let mut lines = vec![
        Line::from(Span::styled(
            format!("Crafting - {}", craft.stations),
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
    ];
    if craft.entries.is_empty() {
        lines.push(Line::from(Span::styled(
            "  no recipes here",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    for (i, row) in rows.iter().enumerate() {
        let selected = i == cursor;
        if selected {
            sel_line = Some(lines.len());
        }
        match row {
            // A collapsible skill header: "▾ Cooking (10)" / "▸ Cooking (10)".
            SectionRow::Header {
                label,
                count,
                collapsed,
                ..
            } => {
                lines.push(section_header_line(label, *count, *collapsed, selected));
            }
            // A recipe under an expanded header.
            SectionRow::Item { index } => {
                let Some(e) = craft.entries.get(*index) else {
                    continue;
                };
                let marker = if selected { ">" } else { " " };
                let name_style = if selected {
                    Style::default()
                        .fg(theme::TEXT_BRIGHT())
                        .patch(theme::selection_style())
                        .add_modifier(Modifier::BOLD)
                } else if e.craftable {
                    Style::default().fg(theme::TEXT())
                } else {
                    Style::default().fg(theme::TEXT_DIM())
                };
                // Name row, with a gated reason when it can't be made.
                let mut name_spans = vec![Span::styled(format!("{marker} {}", e.name), name_style)];
                if !e.craftable && !e.reason.is_empty() {
                    name_spans.push(Span::styled(
                        format!("  ({})", e.reason),
                        Style::default().fg(theme::ERROR()),
                    ));
                }
                lines.push(Line::from(name_spans));
                // What it makes. The panel used to list a name and a pile of
                // ingredients and never once say what came out of them, so
                // "Blessed Oil" or "Wyrmhide Mantle" told a maker nothing.
                if !e.stats.is_empty() {
                    let mut effect = vec![Span::styled(
                        format!("    {}", e.stats),
                        Style::default().fg(theme::AMBER()),
                    )];
                    if let Some(span) = compare_span(e.compare_pct) {
                        effect.push(Span::raw("  "));
                        effect.push(span);
                    }
                    lines.push(Line::from(effect));
                }
                // Ingredient row.
                lines.push(Line::from(Span::styled(
                    format!("    {}", e.inputs),
                    Style::default().fg(theme::TEXT_DIM()),
                )));
            }
        }
    }
    lines.push(Line::raw(""));
    lines.push(hint("w/s", "select  Enter craft/fold"));
    lines.push(hint("u", "close"));
    (lines, sel_line)
}

/// The quest board's picker: every ready-to-claim and still-open bounty for
/// this board, so taking or turning in one is a choice, not a blind draw.
fn board_panel(view: &PlayerView, cursor: usize) -> (Vec<Line<'static>>, Option<usize>) {
    let Some(board) = &view.board else {
        return (
            vec![Line::from(Span::styled(
                "No board here.",
                Style::default().fg(theme::TEXT_DIM()),
            ))],
            None,
        );
    };
    let mut sel_line = None;
    let mut lines = vec![Line::from(Span::styled(
        "Quest Board",
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD),
    ))];
    if board.entries.is_empty() {
        lines.push(Line::from(Span::styled(
            "No bounties posted right now. Make progress on what you're already \
             carrying, or come back later.",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    lines.push(Line::raw(""));
    for (i, e) in board.entries.iter().enumerate() {
        let selected = i == cursor;
        if selected {
            sel_line = Some(lines.len());
        }
        let marker = if selected { ">" } else { " " };
        let (tag, tag_color) = if e.ready {
            ("READY", theme::SUCCESS())
        } else if e.locked {
            ("sealed", theme::ERROR())
        } else {
            ("available", theme::AMBER())
        };
        // A sealed posting reads dim: its hunting ground refuses the player at
        // the door, so it is information, not an offer.
        let base_fg = if e.locked {
            theme::TEXT_DIM()
        } else {
            theme::TEXT_BRIGHT()
        };
        let name_style = if selected {
            Style::default()
                .fg(base_fg)
                .bg(theme::BG_SELECTION())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(base_fg)
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{marker} {}", e.title), name_style),
            Span::styled(format!("  [{tag}]"), Style::default().fg(tag_color)),
            Span::styled(
                format!("  ~Lv{}", e.suggested_level),
                Style::default().fg(theme::AMBER_DIM()),
            ),
        ]));
        lines.push(Line::from(Span::styled(
            format!("    {} ({})", e.blurb, e.objective),
            Style::default().fg(theme::TEXT_DIM()),
        )));
        lines.push(Line::from(Span::styled(
            format!("    {}", e.hint),
            Style::default().fg(theme::TEXT_DIM()),
        )));
        lines.push(Line::from(Span::styled(
            format!("    reward: {}", e.reward),
            Style::default().fg(theme::BADGE_GOLD()),
        )));
    }
    lines.push(Line::raw(""));
    lines.push(hint("w/s", "select"));
    lines.push(hint("Enter", "claim (READY) / accept"));
    lines.push(hint("o", "back"));
    (lines, sel_line)
}

fn stable_panel(view: &PlayerView, cursor: usize) -> (Vec<Line<'static>>, Option<usize>) {
    let Some(stable) = &view.stable else {
        return (
            vec![Line::from(Span::styled(
                "No stable here.",
                Style::default().fg(theme::TEXT_DIM()),
            ))],
            None,
        );
    };
    let mut sel_line = None;
    let mut lines = vec![
        Line::from(Span::styled(
            "The Stable",
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!("your gold: {}", view.gold),
            Style::default().fg(theme::TEXT_DIM()),
        )),
    ];
    // The companion you already keep, if any, shown first as the tend target.
    if let Some(pet) = &view.pet {
        lines.push(Line::from(vec![
            Span::styled(
                format!("{} {} ", pet.glyph, pet.name),
                Style::default()
                    .fg(theme::SUCCESS())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    "Lv{} {}/{}hp{}",
                    pet.level,
                    pet.hp,
                    pet.max_hp,
                    if pet.downed { " (downed)" } else { "" }
                ),
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ]));
    } else {
        lines.push(Line::from(Span::styled(
            "no companion at your heel",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    lines.push(Line::raw(""));
    let row_style = |selected: bool| {
        if selected {
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .patch(theme::selection_style())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_BRIGHT())
        }
    };
    // The kennel leads: the rows Enter calls out, strongest first.
    lines.push(Line::from(Span::styled(
        "Your kennel",
        Style::default().fg(theme::AMBER_GLOW()),
    )));
    if stable.kennel.is_empty() {
        lines.push(Line::from(Span::styled(
            "  empty - tamed and replaced companions rest here",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    for (i, e) in stable.kennel.iter().enumerate() {
        let selected = i == cursor;
        if selected {
            sel_line = Some(lines.len());
        }
        let marker = if selected { ">" } else { " " };
        lines.push(Line::from(vec![
            Span::styled(
                format!("{marker} {} {}", e.glyph, e.name),
                row_style(selected),
            ),
            Span::styled(
                format!(
                    "  Lv{} {}/{}hp · {}atk{}",
                    e.level,
                    e.hp,
                    e.max_hp,
                    e.attack,
                    if e.downed { " (downed)" } else { "" }
                ),
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "For sale",
        Style::default().fg(theme::AMBER_GLOW()),
    )));
    for (i, e) in stable.entries.iter().enumerate() {
        let selected = stable.kennel.len() + i == cursor;
        if selected {
            sel_line = Some(lines.len());
        }
        let marker = if selected { ">" } else { " " };
        let price = if e.owned {
            Span::styled("  owned", Style::default().fg(theme::TEXT_DIM()))
        } else if e.affordable {
            Span::styled(
                format!("  {}g", e.price),
                Style::default().fg(theme::BADGE_GOLD()),
            )
        } else {
            Span::styled(
                format!("  {}g", e.price),
                Style::default().fg(theme::ERROR()),
            )
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("{marker} {} {}", e.glyph, e.name),
                row_style(selected),
            ),
            price,
        ]));
        lines.push(Line::from(Span::styled(
            format!("    {}hp · {}atk", e.hp, e.attack),
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    lines.push(Line::raw(""));
    lines.push(hint("w/s", "select  Enter call out / buy"));
    if let Some(pet) = &view.pet {
        lines.push(hint("G", &format!("feed your pet ({}g)", pet.feed_cost)));
    }
    lines.push(hint("p", "leave stable"));
    (lines, sel_line)
}

/// The Animal Taming panel: the tameable wild beasts roaming this room, each
/// with its required Taming level and the player's odds. Enter attempts the
/// selected tame; success makes the beast your active companion.
fn taming_panel(view: &PlayerView, cursor: usize) -> (Vec<Line<'static>>, Option<usize>) {
    let Some(taming) = &view.taming else {
        return (
            vec![Line::from(Span::styled(
                "No tameable beast roams here.",
                Style::default().fg(theme::TEXT_DIM()),
            ))],
            None,
        );
    };
    let mut sel_line = None;
    let mut lines = vec![
        Line::from(Span::styled(
            "Animal Taming",
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!("your taming level: {}", taming.taming_level),
            Style::default().fg(theme::TEXT_DIM()),
        )),
        Line::raw(""),
    ];
    for (i, e) in taming.entries.iter().enumerate() {
        let selected = i == cursor;
        if selected {
            sel_line = Some(lines.len());
        }
        let marker = if selected { ">" } else { " " };
        let name_style = if selected {
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .patch(theme::selection_style())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_BRIGHT())
        };
        // The odds/reason: green when tamable, red when out of reach or spooked.
        let (status, status_color) = if e.reason.is_empty() {
            (format!("{}% chance", e.odds), theme::SUCCESS())
        } else {
            (e.reason.clone(), theme::ERROR())
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{marker} {} {}", e.glyph, e.name), name_style),
            Span::styled(
                format!("  (need Lv{})", e.req_level),
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ]));
        lines.push(Line::from(Span::styled(
            format!("    {status}"),
            Style::default().fg(status_color),
        )));
    }
    lines.push(Line::raw(""));
    lines.push(hint("w/s", "select  Enter tame"));
    lines.push(hint("q", "leave"));
    (lines, sel_line)
}

/// The whole-world atlas: one row per major region with an exploration meter
/// (where you've been vs. what's unexplored), a boss/loot count, and a danger
/// tier. A region you've never entered reads as undiscovered.
fn atlas_panel(view: &PlayerView) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(Span::styled(
        "World Atlas",
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD),
    ))];

    let total: usize = view.atlas.iter().map(|r| r.total).sum();
    let explored: usize = view.atlas.iter().map(|r| r.explored).sum();
    let pct = explored
        .checked_mul(100)
        .and_then(|n| n.checked_div(total))
        .unwrap_or(0);
    lines.push(Line::from(Span::styled(
        format!("  {explored}/{total} rooms mapped ({pct}%)"),
        Style::default().fg(theme::TEXT_DIM()),
    )));
    lines.push(Line::raw(""));

    for r in &view.atlas {
        let discovered = r.explored > 0;
        let name_color = if !discovered {
            theme::TEXT_FAINT()
        } else if r.explored >= r.total {
            theme::SUCCESS()
        } else {
            theme::TEXT_BRIGHT()
        };
        // Region name + a boss/loot marker (◆ where the great loot lairs).
        let mut head = vec![Span::styled(
            format!("  {}", r.name),
            Style::default().fg(name_color).add_modifier(Modifier::BOLD),
        )];
        if r.bosses > 0 {
            head.push(Span::styled(
                format!("  \u{25C6}{}", r.bosses),
                Style::default().fg(theme::BADGE_GOLD()),
            ));
        }
        // The region's real mob-level band, so the atlas doubles as a "where
        // should I be" chart.
        if let Some((lo, hi)) = r.levels {
            head.push(Span::styled(
                if lo == hi {
                    format!("  Lv {lo}")
                } else {
                    format!("  Lv {lo}-{hi}")
                },
                Style::default().fg(theme::AMBER_DIM()),
            ));
        }
        if r.here {
            head.push(Span::styled(
                "  \u{25C8} you are here",
                Style::default().fg(theme::MENTION()),
            ));
        }
        lines.push(Line::from(head));

        if discovered {
            let bar = meter(r.explored as i32, r.total.max(1) as i32, 12);
            let bar_color = if r.explored >= r.total {
                theme::SUCCESS()
            } else {
                theme::AMBER()
            };
            lines.push(Line::from(vec![
                Span::styled(format!("    {bar} "), Style::default().fg(bar_color)),
                Span::styled(
                    format!("{}/{} · {}", r.explored, r.total, r.tier),
                    Style::default().fg(theme::TEXT_DIM()),
                ),
            ]));
        } else {
            lines.push(Line::from(Span::styled(
                format!("    unexplored · reach via {}", r.note),
                Style::default().fg(theme::TEXT_FAINT()),
            )));
        }
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "  \u{25C6}=bosses (loot)  [ ] scroll  m close",
        Style::default().fg(theme::TEXT_FAINT()),
    )));
    lines
}

fn portal_panel(view: &PlayerView, cursor: usize) -> (Vec<Line<'static>>, Option<usize>) {
    let Some(portal) = &view.portal else {
        return (
            vec![Line::from(Span::styled(
                "No waystone here.",
                Style::default().fg(theme::TEXT_DIM()),
            ))],
            None,
        );
    };
    let mut sel_line = None;
    let mut lines = vec![
        Line::from(Span::styled(
            "The Ways",
            Style::default()
                .fg(theme::MENTION())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "Step through to any waystone you know of.",
            Style::default().fg(theme::TEXT_DIM()),
        )),
    ];
    // A mainland gate you have never stood in is not on the network yet. Say
    // how many are missing without naming them, so the player learns the Ways
    // run further than this list and still has to walk to find out where.
    if portal.unknown_gates > 0 {
        lines.push(Line::from(Span::styled(
            format!(
                "The Ways answer to {} of {} far gates; walk to the rest.",
                portal.known_gates,
                portal.known_gates + portal.unknown_gates
            ),
            Style::default().fg(theme::TEXT_FAINT()),
        )));
    }
    lines.push(Line::raw(""));
    // Known continent gates come first in the destination list, then the
    // villages, then the islands.
    let continent_count = portal.known_gates;
    let village_count = super::archipelago::VILLAGES.len();
    for (i, (label, _room, here)) in portal.entries.iter().enumerate() {
        let selected = i == cursor;
        if selected {
            sel_line = Some(lines.len());
        }
        if i == continent_count {
            lines.push(Line::from(Span::styled(
                "  — the portal villages —",
                Style::default().fg(theme::TEXT_DIM()),
            )));
        }
        if i == continent_count + village_count {
            lines.push(Line::from(Span::styled(
                "  — the Shattered Archipelago —",
                Style::default().fg(theme::TEXT_DIM()),
            )));
        }
        let marker = if selected { ">" } else { " " };
        let suffix = if *here { "  (here)" } else { "" };
        let style = if *here {
            Style::default().fg(theme::TEXT_DIM())
        } else if selected {
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .patch(theme::selection_style())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_BRIGHT())
        };
        let style = if selected && *here {
            style.patch(theme::selection_style())
        } else {
            style
        };
        lines.push(Line::from(Span::styled(
            format!("{marker} {label}{suffix}"),
            style,
        )));
    }
    lines.push(Line::raw(""));
    lines.push(hint("w/s", "select  Enter travel"));
    lines.push(hint("i", "close"));
    (lines, sel_line)
}

fn housing_panel(view: &PlayerView, cursor: usize) -> (Vec<Line<'static>>, Option<usize>) {
    let Some(housing) = &view.housing else {
        return (
            vec![Line::from(Span::styled(
                "No housing ledger here.",
                Style::default().fg(theme::TEXT_DIM()),
            ))],
            None,
        );
    };
    let mut sel_line = None;
    let mut lines = vec![
        Line::from(Span::styled(
            housing.title.clone(),
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!("your gold: {}", view.gold),
            Style::default().fg(theme::TEXT_DIM()),
        )),
        Line::raw(""),
    ];
    for (i, e) in housing.entries.iter().enumerate() {
        let selected = i == cursor;
        if selected {
            sel_line = Some(lines.len());
        }
        let marker = if selected { ">" } else { " " };
        let price_color = if e.owned {
            theme::SUCCESS()
        } else if e.taken {
            theme::TEXT_DIM()
        } else if e.affordable {
            theme::BADGE_GOLD()
        } else {
            theme::ERROR()
        };
        let name_style = if selected {
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .patch(theme::selection_style())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_BRIGHT())
        };
        let price = if e.owned {
            "  (your home)".to_string()
        } else if e.taken {
            "  (claimed)".to_string()
        } else {
            format!("  {}g", e.price)
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{marker} {}", e.name), name_style),
            Span::styled(price, Style::default().fg(price_color)),
        ]));
        lines.push(Line::from(Span::styled(
            format!("    {}", truncate(&e.detail, 46)),
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    lines.push(Line::raw(""));
    lines.push(hint("w/s", "select  Enter buy"));
    lines.push(hint("n", "close ledger"));
    (lines, sel_line)
}

/// Trim a string to `max` chars, adding an ellipsis when cut.
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('\u{2026}');
        out
    }
}

fn appearance_panel(view: &PlayerView, cursor: usize) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(
            "Appearance & Bio",
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
    ];
    // A live portrait of the current choices, so players can preview how each
    // change reshapes their character before committing.
    if view.classed {
        let accent = class_accent(Class::from_key(&view.class_key));
        lines.extend(composed_portrait(
            &view.class_key,
            &view.appearance_idx,
            accent,
        ));
        lines.push(Line::raw(""));
    }
    for (i, (label, value)) in view.appearance.iter().enumerate() {
        let selected = i == cursor;
        let marker = if selected { ">" } else { " " };
        let row_style = if selected {
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .patch(theme::selection_style())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_BRIGHT())
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("{marker} {label:<9} "),
                Style::default().fg(theme::TEXT_DIM()),
            ),
            Span::styled(value.clone(), row_style),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "Your bio:",
        Style::default().fg(theme::TEXT_DIM()),
    )));
    for l in wrap_plain(&view.bio, 30) {
        lines.push(Line::from(Span::styled(
            l,
            Style::default()
                .fg(theme::TEXT())
                .add_modifier(Modifier::ITALIC),
        )));
    }
    lines.push(Line::raw(""));
    lines.push(hint("w/s", "field  Enter next  x prev"));
    lines.push(hint("e", "close"));
    lines
}

/// Word-wrap a plain string to `width` columns.
fn wrap_plain(s: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut line = String::new();
    for word in s.split_whitespace() {
        if !line.is_empty() && line.len() + 1 + word.len() > width {
            out.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        out.push(line);
    }
    out
}

// ---- The footer hint row -------------------------------------------------

/// A bright, room-specific action row: `b  shop here`.
///
/// Louder than `hint`, which is for the standing keys that never change. These
/// are the ones that only work *where you are standing*, so they get the weight.
fn action_hint(key: &str, label: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("  {key:<3}"),
            Style::default()
                .fg(theme::SUCCESS())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {label}"),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
    ])
}

/// What this room offers that nowhere else does: a merchant, a stable, a craft
/// station, a portal, a body to raise. Empty in a room with none of it.
///
/// These lead the panel instead of trailing it. A twenty-line key catalogue at
/// the bottom put the one thing that changes from room to room (there is a shop
/// *here*) below the fold, under fifteen keys that are identical everywhere and
/// already in the `?` guide.
fn room_actions(view: &PlayerView) -> Vec<Line<'static>> {
    let entries = room_action_entries(view);
    if entries.is_empty() {
        return Vec::new();
    }
    let mut out = vec![section("You can")];
    out.extend(entries.iter().map(|(key, label)| action_hint(key, label)));
    out.push(Line::raw(""));
    out
}

/// The feeding line under the companion in the room panel: the key (unless
/// `room_action_entries` already promoted it for a hurt pet, or a healthy
/// pet has no meal to have), the progress a meal buys, and how many of
/// today's meals are left.
fn pet_feed_chips(pet: &PetView) -> Vec<String> {
    let mut chips = Vec::new();
    let healthy = !pet.downed && pet.hp >= pet.max_hp;
    let meal_to_have = pet.level < PET_MAX_LEVEL && pet.meals_today < MEALS_PER_DAY;
    if healthy && meal_to_have {
        chips.push(format!("G feed {}g", pet.feed_cost));
    }
    if pet.level >= PET_MAX_LEVEL {
        chips.push("max level".to_string());
    } else {
        chips.push(format!("{}% to Lv{}", pet.loyalty_pct, pet.level + 1));
        chips.push(format!("{}/{MEALS_PER_DAY} today", pet.meals_today));
    }
    chips
}

/// The `(key, label)` table behind `room_actions`, kept separate because
/// `footer_hints` needs the keys too: a key promoted to the top of the panel is
/// dropped from the dim standing-key block, so each one is shown once, in the
/// loud place, instead of twice in two different styles.
fn room_action_entries(view: &PlayerView) -> Vec<(&'static str, &'static str)> {
    let mut out: Vec<(&'static str, &'static str)> = Vec::new();
    if view.dead {
        return out;
    }
    if view.corpse_here && view.can_resurrect {
        out.push(("g", "raise the fallen"));
    }
    if view.shop.is_some() {
        out.push(("b", "shop here"));
    }
    if view.stable.is_some() {
        out.push(("p", "stable - buy a pet"));
    }
    if view.crafting.is_some() {
        out.push(("u", "craft here"));
    }
    if view.nodes.iter().any(|n| n.gatherable) {
        out.push(("y", "gather here"));
    }
    if view.taming.as_ref().is_some_and(|t| !t.entries.is_empty()) {
        out.push(("q", "tame a beast"));
    }
    if view.portal.is_some() {
        out.push(("i", "the ways - travel"));
    }
    if view.housing.is_some() {
        out.push(("n", "housing ledger"));
    }
    // One key, two errands: `o` opens the look list, and picking the board out
    // of it opens the board. So a board room says so, and any other room with
    // something worth looking at gets the general form - which used to be a
    // key hint sitting *inside* the "Of note" list, styled like one of the
    // features you could walk up to.
    if view.board.is_some() {
        out.push(("o", "read the board"));
    } else if !view.features.is_empty() {
        out.push(("o", "look / interact"));
    }
    // Feeding only leads the panel when it is urgent; a healthy pet's `G`
    // sits under the pet in "Here", beside the loyalty it would raise.
    match &view.pet {
        Some(pet) if pet.downed => out.push(("G", "rouse companion")),
        Some(pet) if pet.hp < pet.max_hp => out.push(("G", "mend companion")),
        Some(_) | None => {}
    }
    out
}

fn footer_hints(view: &PlayerView, width: usize) -> Vec<Line<'static>> {
    let mut lines = vec![section("Commands")];
    if view.dead {
        lines.push(Line::from(Span::styled(
            "  You have fallen.",
            Style::default()
                .fg(theme::ERROR())
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(Span::styled(
            "  Wait for a resurrection,",
            Style::default().fg(theme::TEXT_DIM()),
        )));
        lines.push(hint("r", "release to temple"));
        return lines;
    }
    if view.in_combat_with.is_some() {
        // Mid-fight these are the only keys that matter, and they are worth
        // their lines: everything else waits.
        lines.push(hint("space/x", "strike"));
        lines.push(hint("1-9 0", "use ability"));
        lines.push(hint("z", "flee"));
        lines.push(hint("?", "all keys"));
        return lines;
    }
    // Out of combat this block is the standing keys: identical in every room in
    // the world, so they pack into a few dim lines rather than fifteen. What is
    // specific to *this* room leads the panel instead (`room_actions`), and the
    // full reference lives one keypress away in the `?` guide.
    let mut chips = vec!["wasd move", "space attack", "o look"];
    let has_up = view.exits.iter().any(|(dir, _)| *dir == Dir::Up);
    let has_down = view.exits.iter().any(|(dir, _)| *dir == Dir::Down);
    let has_danger_down = view
        .exits
        .iter()
        .any(|(dir, label)| *dir == Dir::Down && label.contains("dangerous Frontier"));
    match (has_up, has_down) {
        (true, true) => chips.push("< > up/down"),
        (true, false) => chips.push("< up"),
        (false, true) if has_danger_down => chips.push("> the Frontier"),
        (false, true) => chips.push("> down"),
        (false, false) => {}
    }
    // The waypoint used to cost the room panel a standing line ("waypoint set
    // (/ to warp)") that never changed once fixed. It says more here, for no
    // line at all: the warp chip names the zone `/` lands in. `f follow` gives
    // up its place - it is a two-player convoy key that reads fine in the `?`
    // guide, where the waypoint's destination could never be shown.
    let warp = match &view.waypoint {
        Some(zone) => {
            let room = "/ warp ";
            let budget = width.saturating_sub(RAIL_INDENT + room.chars().count());
            format!("{room}{}", truncate(zone, budget))
        }
        None => "/ warp".to_string(),
    };
    chips.extend([
        "c sheet",
        "v abilities",
        "t bag",
        "j quests",
        "k titles",
        "m map",
        "[ ] scroll",
        "r recall",
        "; haven",
        ": waypoint",
        warp.as_str(),
    ]);
    // What the room promoted into "You can" does not repeat down here.
    let promoted = room_action_entries(view);
    chips.retain(|chip| {
        let key = chip.split(' ').next().unwrap_or("");
        !promoted.iter().any(|(k, _)| *k == key)
    });
    for text in pack_hint_chips(&chips, width) {
        lines.push(Line::from(Span::styled(
            format!("  {text}"),
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    lines.push(hint("?", "all keys"));
    lines
}

/// Pack short `key label` chips into `·`-joined lines that fit `width`.
///
/// The standing-key block is rendered into a fixed-width rail with no wrapping
/// (`draw_room_side` paints pre-wrapped lines), so a line built without knowing
/// the width is simply chopped at the edge: `f follow` came out as `f follo`.
/// Packing to the measured width means the same block reads on a 28-column rail
/// and a 60-column one, just in more or fewer lines.
fn pack_hint_chips(chips: &[&str], width: usize) -> Vec<String> {
    const SEP: &str = " · ";
    let budget = width.saturating_sub(RAIL_INDENT).max(1);
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for chip in chips {
        let need = if current.is_empty() {
            UnicodeWidthStr::width(*chip)
        } else {
            UnicodeWidthStr::width(current.as_str())
                + UnicodeWidthStr::width(SEP)
                + UnicodeWidthStr::width(*chip)
        };
        if !current.is_empty() && need > budget {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push_str(SEP);
        }
        current.push_str(chip);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

// ---- The log: wrapping, collapsing, and the recent tail ------------------

fn wrapped_log_tail(view: &PlayerView, width: usize, height: usize) -> Vec<Line<'static>> {
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let mut lines: Vec<Line<'static>> = view
        .log
        .iter()
        .flat_map(|line| wrapped_log_line(line.kind, &line.text, width))
        .collect();
    let start = lines.len().saturating_sub(height);
    lines.split_off(start)
}

fn recent_log_tail(view: &PlayerView, width: usize, height: usize) -> Vec<Line<'static>> {
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let entries = collapsed_recent_entries(view);
    let mut events: Vec<Line<'static>> = entries
        .iter()
        .flat_map(|(kind, text)| wrapped_log_line(*kind, text, width))
        .collect();
    if events.is_empty() {
        events.push(Line::from(Span::styled(
            "  no recent events",
            Style::default().fg(theme::TEXT_FAINT()),
        )));
    }

    // Keep the most recent events that fit, trimming the oldest off the top so
    // the newest line rests at the bottom - a normal MUD feed reads top to
    // bottom, oldest to newest.
    let event_h = height.saturating_sub(1);
    let start = events.len().saturating_sub(event_h);
    let events = events.split_off(start);

    let mut lines = vec![section("Recent")];
    lines.extend(events);
    lines.truncate(height);
    lines
}

/// Recent non-room log entries, consecutive duplicates collapsed to "text (xN)",
/// returned oldest-first. Collapsing walks the log newest-first so a run of
/// repeats folds into a single entry; the list is then flipped back into
/// chronological order for a top-to-bottom MUD feed. Wrapping is left to the
/// caller so multi-line entries never get their wrapped rows reversed.
fn collapsed_recent_entries(view: &PlayerView) -> Vec<(LogKind, String)> {
    let mut entries = Vec::new();
    let mut iter = view
        .log
        .iter()
        .filter(|line| line.kind != LogKind::Room)
        .rev();
    while let Some(line) = iter.next() {
        let mut repeats = 1;
        while let Some(next) = iter.clone().next() {
            if next.kind != line.kind || next.text != line.text {
                break;
            }
            repeats += 1;
            iter.next();
        }
        let text = if repeats > 1 {
            format!("{} (x{repeats})", line.text)
        } else {
            line.text.clone()
        };
        entries.push((line.kind, text));
    }
    entries.reverse();
    entries
}

fn truncate_lines(mut lines: Vec<Line<'static>>, height: u16) -> Vec<Line<'static>> {
    let height = height as usize;
    if lines.len() <= height {
        return lines;
    }
    lines.truncate(height);
    if let Some(last) = lines.last_mut() {
        *last = Line::from(Span::styled(
            "  ...",
            Style::default().fg(theme::TEXT_FAINT()),
        ));
    }
    lines
}

fn separator_line(width: usize) -> Line<'static> {
    let line = if width > 3 {
        format!(" {}", "-".repeat(width.saturating_sub(2)))
    } else {
        "-".repeat(width)
    };
    Line::from(Span::styled(line, Style::default().fg(theme::BORDER())))
}

// ---- The main column's room and battle context ---------------------------

/// The left column's battle frame, shown in place of the room context while a
/// fight is on: the foe's full name and nature, both sides' vitals as wide
/// meters, and every active effect that changes what to press next. The room
/// prose can wait; mid-swing, this is what the column is for.
fn battle_context(view: &PlayerView, width: usize) -> Option<Vec<Line<'static>>> {
    let meter_w = width.saturating_sub(18).clamp(10, 30);
    let (name, name_style, level, traits, hp, max_hp, afflicted) =
        if let Some(mob) = view.mobs.iter().find(|m| m.targeted) {
            let mut traits: Vec<String> = vec![mob.rank.clone()];
            traits.push(format!("strikes with {}", mob.school));
            if let Some(weak) = mob.weak {
                traits.push(format!("weak to {weak}"));
            }
            if let Some(resist) = mob.resist {
                traits.push(format!("shrugs off {resist}"));
            }
            let mut afflicted: Vec<String> = Vec::new();
            if mob.dot_stacks > 0 {
                afflicted.push(format!("bleeding x{}", mob.dot_stacks));
            }
            if mob.stunned {
                afflicted.push("stunned".to_string());
            }
            let marker = if mob.boss { "\u{2021} " } else { "" };
            (
                format!("{marker}{}", mob.name),
                Style::default()
                    .fg(rarity_color(&mob.rank))
                    .add_modifier(Modifier::BOLD),
                mob.level,
                traits,
                mob.hp,
                mob.max_hp,
                afflicted,
            )
        } else {
            let occ = view.occupants.iter().find(|o| o.targeted)?;
            (
                "your rival".to_string(),
                Style::default()
                    .fg(theme::ERROR())
                    .add_modifier(Modifier::BOLD),
                occ.level,
                vec!["duel".to_string()],
                occ.hp,
                occ.max_hp,
                Vec::new(),
            )
        };

    let mut lines = vec![section("Battle")];
    lines.push(Line::from(vec![
        Span::styled(name, name_style),
        Span::styled(
            format!("   Lv{level}"),
            Style::default().fg(theme::TEXT_BRIGHT()),
        ),
    ]));
    lines.push(Line::from(Span::styled(
        format!("  {}", traits.join(" \u{00b7} ")),
        Style::default().fg(theme::TEXT_DIM()),
    )));
    lines.push(Line::from(vec![
        Span::styled("  HP   ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled(
            format!("{} {hp}/{max_hp}", meter(hp, max_hp, meter_w)),
            Style::default().fg(hp_color(hp, max_hp)),
        ),
    ]));
    if !afflicted.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("  afflicted: {}", afflicted.join(" \u{00b7} ")),
            Style::default().fg(theme::SUCCESS()),
        )));
    }
    lines.push(Line::from(Span::styled(
        "\u{2500}".repeat(width.min(60)),
        Style::default().fg(theme::BORDER_DIM()),
    )));
    lines.push(Line::from(vec![
        Span::styled("  You  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled(
            format!(
                "{} {}/{}",
                meter(view.hp, view.max_hp, meter_w),
                view.hp,
                view.max_hp
            ),
            Style::default().fg(hp_color(view.hp, view.max_hp)),
        ),
    ]));
    if view.max_resource > 0 {
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {:<4} ", short_resource(&view.resource_name)),
                Style::default().fg(theme::TEXT_DIM()),
            ),
            Span::styled(
                format!(
                    "{} {}/{}",
                    meter(view.resource, view.max_resource, meter_w),
                    view.resource,
                    view.max_resource
                ),
                Style::default().fg(theme::MENTION()),
            ),
        ]));
    }
    let mut effects: Vec<String> = Vec::new();
    if view.shield > 0 {
        effects.push(format!("shield {}", view.shield));
    }
    if view.empower > 0 {
        effects.push(format!("empowered +{}", view.empower));
    }
    if let Some(coat) = &view.coat {
        effects.push(format!("{} coat x{}", coat.school, coat.charges));
    }
    if view.stunned {
        effects.push("stunned".to_string());
    }
    if !effects.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("  {}", effects.join(" \u{00b7} ")),
            Style::default().fg(theme::AMBER_GLOW()),
        )));
    }
    Some(lines)
}

/// The zone name plus its derived mob-level band ("King's Road · Lv 2-5"),
/// so danger reads at a glance wherever the zone is named.
fn zone_with_band(view: &PlayerView) -> String {
    match view.zone_band {
        Some((lo, hi)) if lo == hi => format!("{} \u{00b7} Lv {lo}", view.zone),
        Some((lo, hi)) => format!("{} \u{00b7} Lv {lo}-{hi}", view.zone),
        None => view.zone.clone(),
    }
}

/// A short label for a class resource, for the battle frame's meter gutter.
fn short_resource(name: &str) -> String {
    let mut label: String = name.chars().take(4).collect();
    label.make_ascii_uppercase();
    label
}

fn current_room_context(view: &PlayerView, width: usize) -> Vec<Line<'static>> {
    let mut lines = vec![
        section("Now"),
        Line::from(vec![
            Span::styled(
                view.room_name.clone(),
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {}", zone_with_band(view)),
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ]),
    ];
    lines.extend(wrap(&view.room_desc, width));

    let exits = if view.exits.is_empty() {
        "none".to_string()
    } else {
        view.exits
            .iter()
            .map(|(_, n)| n.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    lines.push(Line::from(vec![
        Span::styled("  exits ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled(exits, Style::default().fg(theme::AMBER_DIM())),
    ]));

    if !view.mobs.is_empty() {
        lines.push(context_list(
            "foes",
            summarize_names(view.mobs.iter().map(|m| m.name.as_str()), 2),
            theme::ERROR(),
        ));
    }
    if !view.features.is_empty() {
        lines.push(context_list(
            "note",
            summarize_names(view.features.iter().map(|f| f.name.as_str()), 2),
            theme::TEXT_DIM(),
        ));
    }
    if let Some(shop) = &view.shop {
        lines.push(context_list(
            "shop",
            shop.shop_name.clone(),
            theme::SUCCESS(),
        ));
    }
    lines
}

fn context_list(label: &str, value: String, color: ratatui::style::Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("  {label:<5}"),
            Style::default().fg(theme::TEXT_DIM()),
        ),
        Span::styled(value, Style::default().fg(color)),
    ])
}

// ---- Text helpers: wrapping, sections, hints, and vitals -----------------

fn side_kv_wrap(
    label: &str,
    value: &str,
    value_color: ratatui::style::Color,
    width: usize,
) -> Vec<Line<'static>> {
    let label_text = format!("  {label} ");
    let label_width = UnicodeWidthStr::width(label_text.as_str());
    let value_width = width.saturating_sub(label_width).max(1);
    let mut wrapped = wrap_log_text(value, value_width);
    if wrapped.is_empty() {
        wrapped.push(String::new());
    }

    let mut lines = Vec::with_capacity(wrapped.len());
    if let Some(first) = wrapped.first() {
        lines.push(Line::from(vec![
            Span::styled(label_text.clone(), Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                first.trim_start().to_string(),
                Style::default().fg(value_color),
            ),
        ]));
    }
    for line in wrapped.into_iter().skip(1) {
        lines.push(Line::from(vec![
            Span::raw(" ".repeat(label_width)),
            Span::styled(
                line.trim_start().to_string(),
                Style::default().fg(value_color),
            ),
        ]));
    }
    lines
}

fn side_text_wrap(text: &str, color: ratatui::style::Color, width: usize) -> Vec<Line<'static>> {
    side_text_wrap_styled(text, Style::default().fg(color), width)
}

fn side_text_wrap_styled(text: &str, style: Style, width: usize) -> Vec<Line<'static>> {
    let text_width = width.saturating_sub(2).max(1);
    wrap_log_text(text, text_width)
        .into_iter()
        .map(|line| Line::from(Span::styled(format!("  {line}"), style)))
        .collect()
}

fn summarize_names<'a>(names: impl Iterator<Item = &'a str>, visible: usize) -> String {
    let names: Vec<&str> = names.collect();
    let mut text = names
        .iter()
        .take(visible)
        .copied()
        .collect::<Vec<_>>()
        .join(", ");
    let hidden = names.len().saturating_sub(visible);
    if hidden > 0 {
        text.push_str(&format!(" +{hidden} more"));
    }
    text
}

fn wrapped_log_line(kind: LogKind, text: &str, width: usize) -> Vec<Line<'static>> {
    let color = match kind {
        LogKind::Room => LAT_TEXT_DIM,
        LogKind::Travel => theme::AMBER_DIM(),
        LogKind::Normal => LAT_TEXT,
        LogKind::Combat => theme::ERROR(),
        LogKind::System => theme::AMBER_DIM(),
        LogKind::Say => theme::CHAT_BODY(),
        LogKind::Loot => theme::SUCCESS(),
    };
    wrap_log_text(text, width)
        .into_iter()
        .map(|line| Line::from(Span::styled(line, Style::default().fg(color))))
        .collect()
}

fn wrap_log_text(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let continuation = if width > 2 { "  " } else { "" };
    let mut out = Vec::new();
    let mut line = String::new();

    for word in text.split_whitespace() {
        let prefix_only = !continuation.is_empty() && line == continuation;
        let pending_width = UnicodeWidthStr::width(line.as_str());
        let word_width = UnicodeWidthStr::width(word);
        let sep_width = usize::from(!line.is_empty() && !prefix_only);
        if pending_width > 0 && pending_width + sep_width + word_width > width {
            out.push(line);
            line = continuation.to_string();
        }
        if word_width > width {
            append_long_word(&mut out, &mut line, word, width, continuation);
            continue;
        }
        if !line.is_empty() && line != continuation && !line.ends_with(' ') {
            line.push(' ');
        }
        line.push_str(word);
    }

    if line.is_empty() {
        out.push(String::new());
    } else {
        out.push(line);
    }
    out
}

fn append_long_word(
    out: &mut Vec<String>,
    line: &mut String,
    word: &str,
    width: usize,
    continuation: &str,
) {
    if !line.is_empty() && line != continuation {
        out.push(std::mem::take(line));
    }
    if line.is_empty() {
        line.push_str(continuation);
    }

    for ch in word.chars() {
        let ch_width = ch.width().unwrap_or(0);
        let line_width = UnicodeWidthStr::width(line.as_str());
        if line_width > UnicodeWidthStr::width(continuation) && line_width + ch_width > width {
            out.push(std::mem::take(line));
            line.push_str(continuation);
        }
        line.push(ch);
    }
}

/// A spacer line: no spans, or nothing but whitespace in them.
fn is_blank_line(line: &Line<'_>) -> bool {
    line.spans.iter().all(|s| s.content.trim().is_empty())
}

/// Rows to reserve at the floor of the rail for the standing-key block, or
/// `None` when it should stay ordinary scrolling content at the end of the
/// body.
///
/// It is anchored only while it leaves the room itself at least half the rail.
/// Pinning it on a short terminal would trade the dead gap at the bottom of a
/// tall one for something worse: the room's own state squeezed into four rows
/// under a key list that never changes.
fn footer_anchor(footer_rows: usize, height: u16) -> Option<u16> {
    (footer_rows > 0 && footer_rows * 2 <= height as usize).then_some(footer_rows as u16)
}

/// Fit the room panel into `height` rows: the vitals block (everything down to
/// the first spacer) stays pinned at the top, the rest scrolls by `prev_off`,
/// and a `+N more` marker takes the last row whenever content is still hidden
/// below. Returns the lines to render, how many are pinned, and the clamped
/// offset, the last two so the caller can map a line index to its screen row
/// for click targets.
///
/// Pure, because the arithmetic is fiddly: it counts **wrapped rows**, not
/// logical lines (one wildlife entry is three rows in a narrow rail), and an
/// off-by-one here either hides a row with no marker or reports the wrong
/// count.
fn scroll_room_panel(
    mut lines: Vec<Line<'static>>,
    prev_off: usize,
    width: usize,
    height: usize,
) -> (Vec<Line<'static>>, usize, usize) {
    let pinned = lines
        .iter()
        .position(is_blank_line)
        .map(|i| i + 1)
        .unwrap_or(0)
        .min(lines.len());
    let body_h = height.saturating_sub(pinned);
    let body: Vec<Line<'static>> = lines.split_off(pinned);
    let off = scroll_offset(prev_off, &body, None, width, body_h);
    let mut shown: Vec<Line<'static>> = body.into_iter().skip(off).collect();
    let total_rows: usize = shown.iter().map(|line| line_rows(line, width)).sum();
    // Only when something is genuinely below the fold. Reserving the marker row
    // unconditionally pushed the last line off the final page and then reported
    // it as "+1 more", which no amount of scrolling could ever reach.
    if body_h > 0 && total_rows > body_h {
        let room_for = body_h - 1; // the last row is the marker's
        let mut used = 0;
        let mut fits = 0;
        for line in &shown {
            let rows = line_rows(line, width);
            if used + rows > room_for {
                break;
            }
            used += rows;
            fits += 1;
        }
        let hidden = shown.len() - fits;
        if hidden > 0 {
            shown.truncate(fits);
            shown.push(Line::from(Span::styled(
                format!("  +{hidden} more  [ ] to scroll"),
                Style::default()
                    .fg(theme::AMBER_DIM())
                    .add_modifier(Modifier::BOLD),
            )));
        }
    }
    lines.extend(shown);
    (lines, pinned, off)
}

fn section(title: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(" - ", Style::default().fg(theme::BORDER())),
        Span::styled(
            title.to_string(),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
    ])
}

fn stat(label: &str, value: String) -> Line<'static> {
    stat_colored(label, value, theme::TEXT_BRIGHT())
}

/// A `label   value` stat line with the value in a chosen colour.
fn stat_colored(label: &str, value: String, color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("  {label:<7}"),
            Style::default().fg(theme::TEXT_DIM()),
        ),
        Span::styled(value, Style::default().fg(color)),
    ])
}

fn hint(key: &str, label: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {key}"), Style::default().fg(theme::AMBER_DIM())),
        Span::styled(format!("  {label}"), Style::default().fg(theme::TEXT_DIM())),
    ])
}

fn wrap(text: &str, width: usize) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    let mut line = String::from("  ");
    for word in text.split_whitespace() {
        if line.len() + word.len() + 1 > width && !line.trim().is_empty() {
            out.push(Line::from(Span::styled(
                line.clone(),
                Style::default().fg(LAT_TEXT_DIM),
            )));
            line = String::from("  ");
        }
        line.push_str(word);
        line.push(' ');
    }
    if !line.trim().is_empty() {
        out.push(Line::from(Span::styled(
            line,
            Style::default().fg(LAT_TEXT_DIM),
        )));
    }
    out
}

fn short_res(name: &str) -> String {
    name.chars().take(4).collect()
}

fn vital_label(label: &str) -> String {
    format!("{label:<5}")
}

fn hp_color(hp: i32, max_hp: i32) -> ratatui::style::Color {
    if max_hp <= 0 {
        return theme::TEXT_DIM();
    }
    let pct = (hp * 100) / max_hp;
    if pct <= 25 {
        theme::ERROR()
    } else if pct <= 60 {
        theme::AMBER()
    } else {
        theme::SUCCESS()
    }
}

// ---- The follow panel ----------------------------------------------------

/// Follow panel: a selectable list of adventurers in the room. Enter follows the
/// highlighted one (or stops, if you are already following them).
fn follow_panel(
    view: &PlayerView,
    cursor: usize,
    usernames: &UsernameLookup<'_>,
) -> (Vec<Line<'static>>, Option<usize>) {
    let mut lines = vec![section("Follow")];
    let mut sel_line = None;
    if view.occupants.is_empty() {
        lines.push(Line::from(Span::styled(
            "  no one else is here",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    for (i, occ) in view.occupants.iter().enumerate() {
        let name = usernames
            .get(&occ.user_id)
            .cloned()
            .unwrap_or_else(|| "adventurer".to_string());
        let labelled = format!("Lv{:<2} {name}", occ.level);
        let selected = i == cursor;
        if selected {
            sel_line = Some(lines.len());
        }
        let following = view.following == Some(occ.user_id);
        let marker = if selected { "> " } else { "  " };
        let status = if !occ.alive {
            "fallen"
        } else if following {
            "follow"
        } else if occ.in_combat {
            "fight"
        } else {
            ""
        };
        let abbrev = class_abbrev(&occ.class_key);
        let tag = match (status.is_empty(), abbrev.is_empty()) {
            (true, true) => String::new(),
            (true, false) => abbrev.to_string(),
            (false, true) => status.to_string(),
            (false, false) => format!("{status}\u{00b7}{abbrev}"),
        };
        let color = if selected {
            theme::TEXT_BRIGHT()
        } else if !occ.alive {
            theme::ERROR()
        } else if following {
            theme::MENTION()
        } else {
            theme::SUCCESS()
        };
        let weight = if selected {
            Modifier::BOLD
        } else {
            Modifier::empty()
        };
        lines.push(roster_row(
            marker,
            &labelled,
            occ.hp,
            occ.max_hp,
            Style::default().fg(color).add_modifier(weight),
            16,
            &tag,
        ));
    }
    // Profile the highlighted adventurer: show their composed portrait, then bio.
    if let Some(occ) = view.occupants.get(cursor) {
        if !occ.class_key.is_empty() {
            lines.push(Line::raw(""));
            let name = usernames
                .get(&occ.user_id)
                .cloned()
                .unwrap_or_else(|| "adventurer".to_string());
            let accent = class_accent(Class::from_key(&occ.class_key));
            lines.extend(composed_portrait(
                &occ.class_key,
                &occ.appearance_idx,
                accent,
            ));
            lines.push(Line::from(Span::styled(
                format!(" {name}"),
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            )));
        }
        if !occ.bio.is_empty() {
            lines.push(Line::raw(""));
            for l in wrap_plain(&occ.bio, 30) {
                lines.push(Line::from(Span::styled(
                    l,
                    Style::default()
                        .fg(theme::TEXT())
                        .add_modifier(Modifier::ITALIC),
                )));
            }
        }
    }
    lines.push(Line::raw(""));
    lines.push(hint("w/s", "select  Enter follow/stop"));
    if view.following.is_some() {
        lines.push(hint("x", "stop following"));
    }
    lines.push(hint("f", "close"));
    (lines, sel_line)
}

// Lateania reads in its own warm, parchment-toned prose so the world feels
// distinct from the cool chat UI around it. Body/description text uses these;
// headers, rarity, and interactables keep their own accents.

// ---- The Lateania palette, and the kind-to-colour lookups ----------------

const LAT_TEXT: Color = Color::Rgb(0xdc, 0xc9, 0xa4);
const LAT_TEXT_DIM: Color = Color::Rgb(0xac, 0x9b, 0x79);

// The established, widely-understood RPG item-rarity palette — fixed, iconic
// hues so item and creature tiers read at a glance. Intentionally NOT
// theme-driven: this is a convention players already know (common white,
// uncommon green, rare blue, epic purple, legendary orange).
const RARITY_COMMON: Color = Color::Rgb(0xff, 0xff, 0xff);
const RARITY_UNCOMMON: Color = Color::Rgb(0x1e, 0xff, 0x00);
const RARITY_RARE: Color = Color::Rgb(0x00, 0x70, 0xdd);
const RARITY_EPIC: Color = Color::Rgb(0xa3, 0x35, 0xee);
const RARITY_LEGENDARY: Color = Color::Rgb(0xff, 0x80, 0x00);

/// Colour for an item (or a creature, by its rank) in the standard RPG rarity
/// scheme, so tier reads instantly.
fn rarity_color(rarity: &str) -> Color {
    match rarity {
        "uncommon" => RARITY_UNCOMMON,
        "rare" => RARITY_RARE,
        "epic" => RARITY_EPIC,
        "legendary" => RARITY_LEGENDARY,
        // "common" and anything unlabelled read as common white.
        _ => RARITY_COMMON,
    }
}

/// Colour for an interactable room feature, so things you can act on stand out
/// from plain room text the way loot does. A fountain you drink from reads
/// green; vendors and usables you trade/act at read gold; purely lookable
/// scenery (a plaque, a vista) reads a softer cyan "examine me".
fn interactable_color(kind: &str) -> ratatui::style::Color {
    match kind {
        "fountain" => theme::SUCCESS(),
        "bank" | "board" | "stable" | "clerk" => theme::AMBER_GLOW(),
        // Villagers (Genesys) get their own colour so they stand out at a
        // glance from every other lookable thing in the room.
        "villager" => theme::BOT(),
        _ if is_craft_station(kind) => theme::AMBER_GLOW(),
        _ => theme::MENTION(),
    }
}

/// The craft-station feature tags (see `CraftSkill::station`), which read as
/// actionable like the other vendors.
fn is_craft_station(kind: &str) -> bool {
    matches!(
        kind,
        "forge" | "workbench" | "tannery" | "alchemy lab" | "cooking fire"
    )
}

/// Whether a feature kind is something you actively use/trade at (vs. just look
/// at). Drives a brighter, bolder treatment so actionable things pop.
fn is_actionable_feature(kind: &str) -> bool {
    matches!(
        kind,
        "fountain" | "bank" | "board" | "stable" | "clerk" | "villager"
    ) || is_craft_station(kind)
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
