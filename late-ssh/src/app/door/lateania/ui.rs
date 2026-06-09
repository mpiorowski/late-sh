// Rendering for Lateania. Reads the cached per-session snapshot and paints a
// two-column view: the scrolling adventure log on the left, a context side panel
// on the right (room / character / abilities / inventory / shop). Before a class
// is chosen it shows the class-selection screen. Lock-free; never awaits.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::common::theme;
use crate::usernames::UsernameLookup;

use super::{
    classes::Class,
    state::{Panel, State},
    svc::{LogKind, PlayerView},
    world::{Dir, MapCell, MiniMap},
};

const SIDE_WIDE: u16 = 34;
const SIDE_NARROW: u16 = 28;

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, usernames: &UsernameLookup<'_>) {
    let view = state.view();

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
        draw_class_select(frame, area, &view);
        return;
    }

    if area.width < 50 || area.height < 9 {
        draw_compact(frame, area, &view);
        return;
    }

    let side_w = if area.width >= 84 {
        SIDE_WIDE
    } else {
        SIDE_NARROW
    };
    let cols = Layout::horizontal([Constraint::Min(26), Constraint::Length(side_w)]).split(area);
    draw_log(frame, cols[0], &view);
    draw_side(frame, cols[1], state, &view, usernames);
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
    frame.render_widget(
        Paragraph::new(vec![Line::from(vec![Span::styled(
            title,
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )])]),
        rows[0],
    );
    draw_game(frame, rows[1], state, usernames);
}

fn draw_class_select(frame: &mut Frame, area: Rect, view: &PlayerView) {
    let mut lines = vec![
        Line::from(Span::styled(
            "~ LATEANIA ~",
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "Choose your calling. Press its number.",
            Style::default().fg(theme::TEXT_DIM()),
        )),
        Line::raw(""),
        Line::from(Span::styled(
            "Your rolled fate (4d6, drop lowest):",
            Style::default().fg(theme::AMBER()),
        )),
        score_row(view),
        Line::from(Span::styled(
            "Press r to reroll - your scores lock the moment you choose a class.",
            Style::default().fg(theme::TEXT_DIM()),
        )),
        Line::raw(""),
    ];
    for (i, class) in Class::ALL.iter().enumerate() {
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {} ", i + 1),
                Style::default()
                    .fg(theme::BG_CANVAS())
                    .bg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {}  ", class.name()),
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                class.tagline().to_string(),
                Style::default().fg(theme::TEXT()),
            ),
        ]));
        lines.push(Line::from(Span::styled(
            format!(
                "      trait: {} - {}",
                class.trait_name(),
                class.trait_desc()
            ),
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "World by Tasmania - thanks to late.sh and its contributors.",
        Style::default().fg(theme::TEXT_FAINT()),
    )));
    frame.render_widget(Paragraph::new(lines), area);
}

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

fn draw_log(frame: &mut Frame, area: Rect, view: &PlayerView) {
    if area.height < 12 {
        let lines = recent_log_tail(view, area.width as usize, area.height as usize);
        frame.render_widget(Paragraph::new(lines), area);
        return;
    }

    let context_lines = current_room_context(view, area.width as usize);
    let context_h = (context_lines.len() as u16)
        .min(if area.height < 18 { 7 } else { 10 })
        .min(area.height.saturating_sub(4));
    let rows = Layout::vertical([Constraint::Length(context_h), Constraint::Min(1)]).split(area);
    frame.render_widget(
        Paragraph::new(
            context_lines
                .into_iter()
                .take(context_h as usize)
                .collect::<Vec<_>>(),
        ),
        rows[0],
    );

    let events = recent_log_tail(view, rows[1].width as usize, rows[1].height as usize);
    frame.render_widget(Paragraph::new(events), rows[1]);
}

fn draw_side(
    frame: &mut Frame,
    area: Rect,
    state: &State,
    view: &PlayerView,
    usernames: &UsernameLookup<'_>,
) {
    if state.panel() == Panel::Room {
        draw_room_side(frame, area, view, usernames);
        return;
    }

    let lines = match state.panel() {
        Panel::Room => unreachable!("room panel is rendered by draw_room_side"),
        Panel::Character => character_panel(view),
        Panel::Abilities => abilities_panel(view),
        Panel::Inventory => inventory_panel(view, state.cursor()),
        Panel::Shop => shop_panel(view, state.cursor()),
        Panel::Examine => examine_panel(view, state.cursor()),
        Panel::Titles => titles_panel(view, state.cursor()),
        Panel::Quests => quests_panel(view),
        Panel::Follow => follow_panel(view, state.cursor(), usernames),
    };
    frame.render_widget(side_paragraph(lines), area);
}

fn draw_room_side(
    frame: &mut Frame,
    area: Rect,
    view: &PlayerView,
    usernames: &UsernameLookup<'_>,
) {
    let map = minimap_lines(&view.minimap);
    if map.is_empty() {
        frame.render_widget(
            Paragraph::new(room_panel(view, usernames, area.width as usize)),
            area,
        );
        return;
    }

    let map_h = map.len().min(area.height as usize) as u16;
    let rows = Layout::vertical([Constraint::Min(0), Constraint::Length(map_h)]).split(area);
    frame.render_widget(
        Paragraph::new(room_panel(view, usernames, rows[0].width as usize)),
        rows[0],
    );
    frame.render_widget(Paragraph::new(map), rows[1]);
}

/// Titles panel: a selectable list of earned titles with their levels. Enter
/// sets the highlighted one as your displayed title (or clears it).
fn titles_panel(view: &PlayerView, cursor: usize) -> Vec<Line<'static>> {
    let mut lines = vec![section("Titles")];
    if view.titles.is_empty() {
        lines.push(Line::from(Span::styled(
            "  none earned yet - slay notable foes",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    for (i, title) in view.titles.iter().enumerate() {
        let selected = i == cursor;
        let active = view.active_title == Some(i);
        let level = view.title_levels.get(i).copied().unwrap_or(1);
        let marker = if selected { ">" } else { " " };
        let active_tag = if active { " *" } else { "" };
        let style = if selected {
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .bg(theme::BG_SELECTION())
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
    lines
}

/// Quest journal: the Frontier zone quests and whether each has been cleared.
fn quests_panel(view: &PlayerView) -> Vec<Line<'static>> {
    let mut lines = vec![section("Quest Journal")];
    let done = view.quests.iter().filter(|q| q.done).count();
    lines.push(Line::from(Span::styled(
        format!("  {done}/{} zones cleared", view.quests.len()),
        Style::default().fg(theme::TEXT_DIM()),
    )));
    lines.push(Line::raw(""));
    for q in &view.quests {
        let (mark, color) = if q.done {
            ("[x]", theme::SUCCESS())
        } else {
            ("[ ]", theme::AMBER())
        };
        lines.push(Line::from(Span::styled(
            format!("{mark} {}", q.name),
            Style::default().fg(color),
        )));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "  reward: the \"Champion of ...\"",
        Style::default().fg(theme::TEXT_DIM()),
    )));
    lines.push(Line::from(Span::styled(
        "  title (Lv = boss) + a bounty",
        Style::default().fg(theme::TEXT_DIM()),
    )));
    lines.push(Line::raw(""));
    lines.push(hint("j", "close"));
    lines
}

fn vitals(view: &PlayerView) -> Vec<Line<'static>> {
    vec![
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
        Line::from(vec![
            Span::styled(vital_label("HP"), Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                format!("{}/{}", view.hp, view.max_hp),
                Style::default()
                    .fg(hp_color(view.hp, view.max_hp))
                    .add_modifier(Modifier::BOLD),
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
        Line::from(vec![
            Span::styled(vital_label("gold"), Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                format!("{}", view.gold),
                Style::default().fg(theme::BADGE_GOLD()),
            ),
        ]),
    ]
}

fn room_panel(
    view: &PlayerView,
    usernames: &UsernameLookup<'_>,
    width: usize,
) -> Vec<Line<'static>> {
    let mut lines = vitals(view);
    lines.push(Line::raw(""));
    lines.push(section("Here"));
    lines.extend(side_text_wrap(&view.zone, theme::TEXT(), width));
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
    if !view.features.is_empty() {
        lines.push(section("Of note"));
        for feat in &view.features {
            lines.extend(side_text_wrap(
                feat.name.as_str(),
                interactable_color(&feat.kind),
                width,
            ));
        }
        lines.push(hint("o", "look / interact"));
    }
    if !view.mobs.is_empty() {
        lines.push(section("Foes"));
        for mob in &view.mobs {
            let mut name_style = Style::default().fg(rarity_color(&mob.rank));
            let marker = if mob.boss {
                name_style = name_style.add_modifier(Modifier::BOLD);
                "‡ "
            } else {
                "  "
            };
            let text = format!(
                "{marker}Lv{} {} {}/{}",
                mob.level, mob.name, mob.hp, mob.max_hp
            );
            lines.extend(side_text_wrap_styled(&text, name_style, width));
        }
    }
    if !view.occupants.is_empty() {
        lines.push(section("Adventurers here"));
        for occ in &view.occupants {
            let name = usernames
                .get(&occ.user_id)
                .cloned()
                .unwrap_or_else(|| "adventurer".to_string());
            let following = view.following == Some(occ.user_id);
            let tag = if following {
                " (following)"
            } else if occ.in_combat {
                " (fighting)"
            } else {
                ""
            };
            let color = if following {
                theme::MENTION()
            } else {
                theme::SUCCESS()
            };
            lines.extend(side_text_wrap(&format!("{name}{tag}"), color, width));
        }
    }
    if !view.wildlife.is_empty() {
        lines.push(section("Wildlife"));
        for w in &view.wildlife {
            let (marker, color) = match w.kind.as_str() {
                "boon" => ("✦ ", theme::BADGE_GOLD()),
                "huntable" => ("» ", theme::AMBER()),
                _ => ("~ ", theme::TEXT_DIM()),
            };
            let detail = if !w.perk.is_empty() {
                format!(" — a boon ({})", w.perk)
            } else if w.kind == "huntable" {
                " — game (attack to hunt)".to_string()
            } else {
                String::new()
            };
            lines.extend(side_text_wrap(
                &format!("{marker}{}{detail}", w.name),
                color,
                width,
            ));
        }
    }
    lines.push(Line::raw(""));
    lines.extend(footer_hints(view));
    lines
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
        MapCell::ConnSlash => ('/', theme::BORDER()),
        MapCell::ConnBack => ('\\', theme::BORDER()),
        MapCell::ConnCross => ('X', theme::BORDER()),
        MapCell::TrailH => ('-', theme::AMBER_GLOW()),
        MapCell::TrailV => ('|', theme::AMBER_GLOW()),
        MapCell::TrailSlash => ('/', theme::AMBER_GLOW()),
        MapCell::TrailBack => ('\\', theme::AMBER_GLOW()),
        MapCell::TrailCross => ('X', theme::AMBER_GLOW()),
    };
    let mut style = Style::default().fg(color);
    if matches!(cell, MapCell::Current | MapCell::Previous) {
        style = style.add_modifier(Modifier::BOLD);
    }
    Span::styled(glyph.to_string(), style)
}

fn character_panel(view: &PlayerView) -> Vec<Line<'static>> {
    let mut lines = vitals(view);
    lines.push(Line::raw(""));
    lines.push(section("Combat"));
    lines.push(stat("attack", view.attack.to_string()));
    lines.push(stat("armor", view.armor.to_string()));
    lines.push(Line::raw(""));
    lines.push(section("Scores"));
    lines.push(score_row(view));
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
    lines.push(hint("c", "close  v abilities  t bag"));
    lines
}

/// Examine panel: the lookable things in the current room.
fn examine_panel(view: &PlayerView, cursor: usize) -> Vec<Line<'static>> {
    let mut lines = vec![section("Look at")];
    if view.features.is_empty() {
        lines.push(Line::from(Span::styled(
            "  nothing of note here",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    for (i, feat) in view.features.iter().enumerate() {
        let selected = i == cursor;
        let marker = if selected { ">" } else { " " };
        let tag = if feat.kind.is_empty() {
            String::new()
        } else {
            format!(" [{}]", feat.kind)
        };
        let style = if selected {
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .bg(theme::BG_SELECTION())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(interactable_color(&feat.kind))
        };
        lines.push(Line::from(Span::styled(
            format!("{marker} {}{}", feat.name, tag),
            style,
        )));
    }
    lines.push(Line::raw(""));
    lines.push(hint("w/s", "select  Enter look"));
    lines.push(hint("o", "close"));
    lines
}

/// One compact line of the six ability scores with their modifiers.
fn score_row(view: &PlayerView) -> Line<'static> {
    let mut spans = vec![Span::raw("  ")];
    for (label, value, modifier) in view.scores.rows() {
        let sign = if modifier >= 0 { "+" } else { "" };
        spans.push(Span::styled(
            format!("{label} "),
            Style::default().fg(theme::TEXT_DIM()),
        ));
        spans.push(Span::styled(
            format!("{value}({sign}{modifier}) "),
            Style::default().fg(theme::TEXT_BRIGHT()),
        ));
    }
    Line::from(spans)
}

fn abilities_panel(view: &PlayerView) -> Vec<Line<'static>> {
    let mut lines = vec![section("Abilities")];
    if view.abilities.is_empty() {
        lines.push(Line::from(Span::styled(
            "  none yet",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    for a in &view.abilities {
        let color = if a.ready {
            theme::TEXT_BRIGHT()
        } else {
            theme::TEXT_FAINT()
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {} ", a.slot),
                Style::default().fg(theme::BG_CANVAS()).bg(if a.ready {
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
    lines.push(hint("1-9", "use ability in combat"));
    lines.push(hint("v", "close"));
    lines
}

fn inventory_panel(view: &PlayerView, cursor: usize) -> Vec<Line<'static>> {
    let mut lines = vec![
        section("Inventory"),
        Line::from(Span::styled(
            format!("  {} gold", view.gold),
            Style::default().fg(theme::BADGE_GOLD()),
        )),
    ];
    if view.inventory.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (empty)",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    for (i, it) in view.inventory.iter().enumerate() {
        let selected = i == cursor;
        let marker = if selected { ">" } else { " " };
        let tag = if it.equipped {
            " [worn]".to_string()
        } else if let Some(slot) = &it.slot {
            format!(" ({slot})")
        } else {
            String::new()
        };
        let style = if selected {
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .bg(theme::BG_SELECTION())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(rarity_color(&it.rarity))
        };
        let mut spans = vec![Span::styled(format!("{marker} {}{}", it.name, tag), style)];
        if !it.stats.is_empty() {
            spans.push(Span::styled(
                format!("  {}", it.stats),
                Style::default().fg(theme::TEXT_DIM()),
            ));
        }
        lines.push(Line::from(spans));
    }
    lines.push(Line::raw(""));
    lines.push(hint("w/s", "select  Enter equip/use"));
    lines.push(hint("x", "sell (at a shop)  t close"));
    lines
}

fn shop_panel(view: &PlayerView, cursor: usize) -> Vec<Line<'static>> {
    let Some(shop) = &view.shop else {
        return vec![Line::from(Span::styled(
            "No shop here.",
            Style::default().fg(theme::TEXT_DIM()),
        ))];
    };
    let mut lines = vec![
        Line::from(Span::styled(
            shop.shop_name.clone(),
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!("{} - your gold: {}", shop.npc_name, view.gold),
            Style::default().fg(theme::TEXT_DIM()),
        )),
        Line::raw(""),
    ];
    for (i, e) in shop.entries.iter().enumerate() {
        let selected = i == cursor;
        let marker = if selected { ">" } else { " " };
        let price_color = if e.affordable {
            theme::BADGE_GOLD()
        } else {
            theme::ERROR()
        };
        let name_style = if selected {
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .bg(theme::BG_SELECTION())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(rarity_color(&e.rarity))
        };
        let mut spans = vec![Span::styled(format!("{marker} {}", e.name), name_style)];
        if !e.stats.is_empty() {
            spans.push(Span::styled(
                format!("  {}", e.stats),
                Style::default().fg(theme::TEXT_DIM()),
            ));
        }
        spans.push(Span::styled(
            format!("  {}g", e.price),
            Style::default().fg(price_color),
        ));
        lines.push(Line::from(spans));
    }
    lines.push(Line::raw(""));
    lines.push(hint("w/s", "select  Enter buy"));
    lines.push(hint("b", "leave shop"));
    lines
}

fn footer_hints(view: &PlayerView) -> Vec<Line<'static>> {
    let mut lines = vec![section("Commands")];
    if view.respawning {
        lines.push(Line::from(Span::styled(
            "  recovering...",
            Style::default().fg(theme::TEXT_DIM()),
        )));
        return lines;
    }
    if view.in_combat_with.is_some() {
        lines.push(hint("space/x", "strike"));
        lines.push(hint("1-9", "use ability"));
        lines.push(hint("z", "flee"));
    } else {
        lines.push(hint("wasd/arrows", "move"));
        lines.push(hint("yunm", "diagonals"));
        // Vertical exits aren't on the wasd/diagonal keys, so spell out the
        // stair keys - but only when this room actually has a way up or down,
        // so the hint appears exactly when the player needs it.
        let has_up = view.exits.iter().any(|(dir, _)| *dir == Dir::Up);
        let has_down = view.exits.iter().any(|(dir, _)| *dir == Dir::Down);
        match (has_up, has_down) {
            (true, true) => lines.push(hint("< >", "climb up / go down")),
            (true, false) => lines.push(hint("<", "climb up")),
            (false, true) => lines.push(hint(">", "go down")),
            (false, false) => {}
        }
        lines.push(hint("space", "attack"));
        lines.push(hint("o", "look at things"));
    }
    lines.push(hint("c v t", "sheet abilities bag"));
    lines.push(hint("j k", "quests titles"));
    lines.push(hint("r f", "recall follow"));
    if view.shop.is_some() {
        lines.push(hint("b", "shop"));
    }
    lines.push(hint("Esc", "leave"));
    lines
}

// ---- helpers -------------------------------------------------------------

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

    let mut events: Vec<Line<'static>> = view
        .log
        .iter()
        .filter(|line| line.kind != LogKind::Room)
        .flat_map(|line| wrapped_log_line(line.kind, &line.text, width))
        .collect();
    if events.is_empty() {
        events.push(Line::from(Span::styled(
            "  no recent events",
            Style::default().fg(theme::TEXT_FAINT()),
        )));
    }

    events.reverse();
    let event_h = height.saturating_sub(1);
    let mut lines = vec![section("Recent")];
    lines.extend(events.into_iter().take(event_h));
    lines.truncate(height);
    lines
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
                format!("  {}", view.zone),
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ]),
    ];
    lines.extend(limited_wrap(&view.room_desc, width, 4));

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

fn limited_wrap(text: &str, width: usize, max_lines: usize) -> Vec<Line<'static>> {
    let mut lines = wrap(text, width);
    if lines.len() > max_lines {
        lines.truncate(max_lines);
        if let Some(last) = lines.last_mut() {
            *last = Line::from(Span::styled(
                "  ...",
                Style::default().fg(theme::TEXT_FAINT()),
            ));
        }
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
        LogKind::Room => theme::TEXT_DIM(),
        LogKind::Travel => theme::AMBER_DIM(),
        LogKind::Normal => theme::TEXT(),
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
    Line::from(vec![
        Span::styled(
            format!("  {label:<7}"),
            Style::default().fg(theme::TEXT_DIM()),
        ),
        Span::styled(value, Style::default().fg(theme::TEXT_BRIGHT())),
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
                Style::default().fg(theme::TEXT_DIM()),
            )));
            line = String::from("  ");
        }
        line.push_str(word);
        line.push(' ');
    }
    if !line.trim().is_empty() {
        out.push(Line::from(Span::styled(
            line,
            Style::default().fg(theme::TEXT_DIM()),
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

/// Follow panel: a selectable list of adventurers in the room. Enter follows the
/// highlighted one (or stops, if you are already following them).
fn follow_panel(
    view: &PlayerView,
    cursor: usize,
    usernames: &UsernameLookup<'_>,
) -> Vec<Line<'static>> {
    let mut lines = vec![section("Follow")];
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
        let selected = i == cursor;
        let following = view.following == Some(occ.user_id);
        let marker = if selected { ">" } else { " " };
        let tag = if following { " (following)" } else { "" };
        let style = if selected {
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .bg(theme::BG_SELECTION())
                .add_modifier(Modifier::BOLD)
        } else if following {
            Style::default().fg(theme::MENTION())
        } else {
            Style::default().fg(theme::SUCCESS())
        };
        lines.push(Line::from(Span::styled(
            format!("{marker} {name}{tag}"),
            style,
        )));
    }
    lines.push(Line::raw(""));
    lines.push(hint("w/s", "select  Enter follow/stop"));
    if view.following.is_some() {
        lines.push(hint("x", "stop following"));
    }
    lines.push(hint("f", "close"));
    lines
}

fn rarity_color(rarity: &str) -> ratatui::style::Color {
    match rarity {
        "uncommon" => theme::SUCCESS(),
        "rare" => theme::MENTION(),
        "epic" => theme::AMBER_GLOW(),
        "legendary" => theme::BADGE_GOLD(),
        _ => theme::TEXT(),
    }
}

/// Colour for an interactable room feature, so things you can act on stand out
/// from plain room text: usable things (a fountain you can drink from) read
/// green; everything else you can examine reads cyan.
fn interactable_color(kind: &str) -> ratatui::style::Color {
    match kind {
        "fountain" => theme::SUCCESS(),
        _ => theme::MENTION(),
    }
}
