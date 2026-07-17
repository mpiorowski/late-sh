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

    if area.width < 50 || area.height < 9 {
        draw_compact(frame, area, &view);
        return;
    }

    // The character sheet expands to the full view when there is room, for a
    // dense dashboard (portrait, dot-rated scores, vitals bars). It falls back
    // to the narrow side panel on cramped terminals.
    if state.panel() == Panel::Character && area.width >= 72 && area.height >= 18 {
        draw_character_sheet(frame, area, &view);
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
    // While composing a chat line, reserve the bottom row for the say prompt.
    if let Some(text) = state.chat_text() {
        let body = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(rows[1]);
        draw_game(frame, body[0], state, usernames);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    "Say: ",
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
            body[1],
        );
    } else {
        draw_game(frame, rows[1], state, usernames);
    }
}

fn draw_class_select(frame: &mut Frame, area: Rect, view: &PlayerView, cursor: usize) {
    let cursor = cursor.min(Class::ALL.len() - 1);
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
        score_row(view),
        Line::raw(""),
    ];
    // One compact row per class; the highlighted one is expanded below.
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
                .bg(theme::BG_SELECTION())
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
    }
    // Detail panel for the highlighted class.
    let chosen = Class::ALL[cursor];
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled(
            format!("{} ", chosen.name()),
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
        format!("  {}", chosen.trait_desc()),
        Style::default().fg(theme::TEXT()),
    )));
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "World by Tasmania - thanks to late.sh and its contributors.",
        Style::default().fg(theme::TEXT_FAINT()),
    )));
    frame.render_widget(Paragraph::new(lines), area);
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
        draw_room_side(frame, area, view, usernames);
        return;
    }

    // List panels return the line index of the highlighted row so the view can
    // scroll to keep the selection visible; text panels return `None`.
    let (lines, selected) = match state.panel() {
        Panel::Room => unreachable!("room panel is rendered by draw_room_side"),
        Panel::Character => (character_panel(view), None),
        Panel::Abilities => abilities_panel(view, state.cursor()),
        Panel::Inventory => inventory_panel(view, state.cursor()),
        Panel::Shop => shop_panel(view, state.cursor()),
        Panel::Examine => examine_panel(view, state.cursor()),
        Panel::Titles => titles_panel(view, state.cursor()),
        Panel::Quests => (quests_panel(view), None),
        Panel::Follow => follow_panel(view, state.cursor(), usernames),
        Panel::Stable => stable_panel(view, state.cursor()),
        Panel::Taming => taming_panel(view, state.cursor()),
        Panel::Housing => housing_panel(view, state.cursor()),
        Panel::Portal => portal_panel(view, state.cursor()),
        Panel::Appearance => (appearance_panel(view, state.cursor()), None),
        Panel::Crafting => crafting_panel(view, state.cursor()),
        Panel::Map => (atlas_panel(view), None),
    };
    let off = scroll_offset(
        state.list_scroll(),
        lines.len(),
        selected,
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

/// New first-visible line for a side panel, given the previous scroll `prev`.
///
/// List panels pass the highlighted row as `selected` and auto-follow it,
/// nudging only when it would come within `LIST_SCROLL_MARGIN` of an edge so
/// the list scrolls under the cursor. Cursor-less text panels pass
/// `selected = None` and are scrolled manually (`[` / `]`); here we just clamp
/// the requested offset to the content.
fn scroll_offset(prev: usize, total: usize, selected: Option<usize>, height: usize) -> usize {
    if height == 0 || total <= height {
        return 0;
    }
    let max = total - height;
    let Some(sel) = selected else {
        // Text panel: honor the manual offset, clamped to the content.
        return prev.min(max);
    };
    // Margin can't exceed what fits above and below within the window.
    let margin = LIST_SCROLL_MARGIN.min(height.saturating_sub(1) / 2);
    let mut off = prev.min(max);
    if sel < off + margin {
        off = sel.saturating_sub(margin);
    } else if sel + margin >= off + height {
        off = sel + margin + 1 - height;
    }
    off.min(max)
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
    (lines, sel_line)
}

/// Quest journal: Frontier zone clears plus active board bounties.
fn quests_panel(view: &PlayerView) -> Vec<Line<'static>> {
    let mut lines = vec![section("Quest Journal")];
    let frontier_total = view.quests.iter().filter(|q| q.frontier).count();
    let done = view.quests.iter().filter(|q| q.frontier && q.done).count();
    lines.push(Line::from(Span::styled(
        format!("  {done}/{frontier_total} zones cleared"),
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
        "  Frontier: Champion title + bounty",
        Style::default().fg(theme::TEXT_DIM()),
    )));
    lines.push(Line::from(Span::styled(
        "  Boards: claim ready bounties at their post",
        Style::default().fg(theme::TEXT_DIM()),
    )));
    lines.push(Line::raw(""));
    lines.push(hint("j", "close"));
    lines.push(hint("[ ]", "scroll"));
    lines
}

fn vitals(view: &PlayerView) -> Vec<Line<'static>> {
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

fn room_panel(
    view: &PlayerView,
    usernames: &UsernameLookup<'_>,
    width: usize,
) -> Vec<Line<'static>> {
    let mut lines = vitals(view);
    lines.push(Line::raw(""));
    lines.push(section("Here"));
    lines.extend(side_text_wrap(&view.zone, theme::TEXT(), width));
    // The living-world clock: time of day and weather.
    lines.push(Line::from(Span::styled(
        format!("  {} · {}", view.time_of_day, view.weather),
        Style::default().fg(theme::AMBER_DIM()),
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
        lines.push(hint("o", "look / interact"));
    }
    if !view.mobs.is_empty() {
        lines.push(section("Foes"));
        // Reserve room for the leading `Lv## ` and the HP meter, so foe names
        // get whatever width is left in the panel.
        let name_w = (width.saturating_sub(13)).clamp(6, 16);
        for mob in &view.mobs {
            let (marker, weight) = if mob.boss {
                ("‡ ", Modifier::BOLD)
            } else {
                ("  ", Modifier::empty())
            };
            let labelled = format!("Lv{:<2} {}", mob.level, mob.name);
            lines.push(roster_row(
                marker,
                &labelled,
                mob.hp,
                mob.max_hp,
                Style::default()
                    .fg(rarity_color(&mob.rank))
                    .add_modifier(weight),
                name_w,
                "",
            ));
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
            let (tag, color) = if !occ.alive {
                ("fallen", theme::ERROR())
            } else if following {
                ("follow", theme::MENTION())
            } else if occ.in_combat {
                ("fight", theme::AMBER())
            } else {
                ("", theme::SUCCESS())
            };
            let tag_w = if tag.is_empty() {
                0
            } else {
                1 + UnicodeWidthStr::width(tag)
            };
            let name_w = width.saturating_sub(9 + tag_w).clamp(6, 16);
            lines.push(roster_row(
                "  ",
                &name,
                occ.hp,
                occ.max_hp,
                Style::default().fg(color),
                name_w,
                tag,
            ));
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
                format!(", a boon ({})", w.perk)
            } else if w.kind == "huntable" {
                ", game (attack to hunt)".to_string()
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
        MapCell::TrailH => ('-', theme::AMBER_GLOW()),
        MapCell::TrailV => ('|', theme::AMBER_GLOW()),
    };
    let mut style = Style::default().fg(color);
    if matches!(cell, MapCell::Current | MapCell::Previous) {
        style = style.add_modifier(Modifier::BOLD);
    }
    Span::styled(glyph.to_string(), style)
}

/// Full-width character dashboard (the `c` panel when the terminal is roomy).
/// A class portrait and vitals bars on the left, ability scores as dot ratings
/// in the middle, and combat/derived stats, trait, titles, and XP on the right.
fn draw_character_sheet(frame: &mut Frame, area: Rect, view: &PlayerView) {
    let accent = class_accent(&view.class_name);
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

    frame.render_widget(Paragraph::new(sheet_identity(view, accent)), cols[0]);
    frame.render_widget(
        Paragraph::new(sheet_attributes(view, accent)).wrap(Wrap { trim: false }),
        cols[1],
    );
    frame.render_widget(
        Paragraph::new(sheet_derived(view, accent)).wrap(Wrap { trim: false }),
        cols[2],
    );
}

/// Left column: portrait, identity headline, and vitals as filled meters.
fn sheet_identity(view: &PlayerView, accent: Color) -> Vec<Line<'static>> {
    let mut lines = class_portrait(&view.class_name, accent);
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
fn sheet_attributes(view: &PlayerView, accent: Color) -> Vec<Line<'static>> {
    let mut lines = vec![section("Attributes")];
    let primary = primary_label(&view.class_name);
    for (label, value, modifier) in view.scores.rows() {
        let sign = if modifier >= 0 { "+" } else { "" };
        let is_primary = label == primary;
        let (label_color, dot_color, weight) = if is_primary {
            (accent, accent, Modifier::BOLD)
        } else {
            (theme::TEXT_DIM(), theme::CHAT_BODY(), Modifier::empty())
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {label} "),
                Style::default().fg(label_color).add_modifier(weight),
            ),
            Span::styled(
                format!("{value:>2}({sign}{modifier}) "),
                Style::default().fg(theme::TEXT_BRIGHT()),
            ),
            Span::styled(score_dots(value), Style::default().fg(dot_color)),
        ]));
    }
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

/// Right column: combat numbers, revives, earned titles, and the XP meter.
fn sheet_derived(view: &PlayerView, accent: Color) -> Vec<Line<'static>> {
    let mut lines = vec![section("Combat")];
    lines.push(stat("attack", format!("+{}", view.attack)));
    lines.push(stat("armor", view.armor.to_string()));
    if view.resurrection_cap > 0 {
        lines.push(stat(
            "revives",
            format!("{}/{}", view.resurrections_left, view.resurrection_cap),
        ));
    }
    lines.push(Line::raw(""));
    lines.push(section("Titles"));
    if view.titles.is_empty() {
        lines.push(Line::from(Span::styled(
            "  none yet",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    for title in &view.titles {
        lines.push(Line::from(Span::styled(
            format!("  {title}"),
            Style::default().fg(theme::BADGE_GOLD()),
        )));
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
    lines.push(hint("c", "close  v abilities  t bag"));
    lines
}

/// The gathering-trades block: each skill's level and a compact progress
/// readout. Shown on both the full character sheet and the narrow panel; the `y`
/// hint teaches how to work a node.
fn skills_block(view: &PlayerView) -> Vec<Line<'static>> {
    let mut lines = vec![section("Trades")];
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

/// An ability score (4d6 range 3..=18) as an 8-cell dot rating.
fn score_dots(value: i32) -> String {
    const WIDTH: i32 = 8;
    let filled = (value.clamp(0, 18) * WIDTH / 18).clamp(0, WIDTH);
    (0..WIDTH)
        .map(|i| if i < filled { '●' } else { '○' })
        .collect()
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

/// The short ability-score label of a class's primary score, for highlighting.
fn primary_label(class_name: &str) -> &'static str {
    match class_name {
        "Warrior" => "STR",
        "Mage" => "INT",
        "Cleric" => "WIS",
        "Rogue" | "Ranger" => "DEX",
        _ => "",
    }
}

/// The accent colour that tints a class's portrait and headline.
fn class_accent(class_name: &str) -> Color {
    match class_name {
        "Warrior" => theme::AMBER(),
        "Mage" => theme::MENTION(),
        "Cleric" => theme::BADGE_GOLD(),
        "Rogue" => theme::ERROR(),
        "Ranger" => theme::SUCCESS(),
        _ => theme::TEXT_BRIGHT(),
    }
}

/// A single-glyph class emblem shown beneath the portrait bust.
fn class_emblem(class_name: &str) -> &'static str {
    match class_name {
        "Warrior" => "⚔ Warrior",
        "Mage" => "✦ Mage",
        "Cleric" => "✚ Cleric",
        "Rogue" => "† Rogue",
        "Ranger" => "➹ Ranger",
        _ => "Adventurer",
    }
}

/// A small ASCII portrait bust, accent-tinted, with the class emblem below it.
/// The bust art is shared; the emblem and colour give each class its identity.
fn class_portrait(class_name: &str, accent: Color) -> Vec<Line<'static>> {
    const BUST: [&str; 6] = [
        "  ▄█████▄ ",
        " ██▀───▀██",
        " █  ◉ ◉  █",
        " █   ▾   █",
        " ██  ◡  ██",
        "  ▀█████▀ ",
    ];
    let mut lines: Vec<Line<'static>> = BUST
        .iter()
        .map(|row| {
            Line::from(Span::styled(
                (*row).to_string(),
                Style::default().fg(accent),
            ))
        })
        .collect();
    lines.push(Line::from(Span::styled(
        format!(" {}", class_emblem(class_name)),
        Style::default().fg(accent).add_modifier(Modifier::BOLD),
    )));
    lines
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
                .bg(theme::BG_SELECTION())
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
    lines.push(hint("Enter", "cast selected  1-9 cast that slot"));
    lines.push(hint("0", "casts slot 10 while adventuring"));
    lines.push(hint("v", "close"));
    (lines, sel_line)
}

fn inventory_panel(view: &PlayerView, cursor: usize) -> (Vec<Line<'static>>, Option<usize>) {
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
    for (i, it) in view.inventory.iter().enumerate() {
        let selected = i == cursor;
        if selected {
            sel_line = Some(lines.len());
        }
        let marker = if selected { ">" } else { " " };
        let tag = inventory_item_tag(it.equipped, it.slot.as_deref());
        let style = if selected {
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .bg(theme::BG_SELECTION())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(rarity_color(&it.rarity))
        };
        let spans = vec![Span::styled(format!("{marker} {}{}", it.name, tag), style)];
        lines.push(Line::from(spans));
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
    }
    lines.push(Line::raw(""));
    lines.push(hint("w/s", "select  Enter equip/use"));
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

fn shop_panel(view: &PlayerView, cursor: usize) -> (Vec<Line<'static>>, Option<usize>) {
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
    for (i, e) in shop.entries.iter().enumerate() {
        let selected = i == cursor;
        if selected {
            sel_line = Some(lines.len());
        }
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
            continue;
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
    (lines, sel_line)
}

fn crafting_panel(view: &PlayerView, cursor: usize) -> (Vec<Line<'static>>, Option<usize>) {
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
    for (i, e) in craft.entries.iter().enumerate() {
        let selected = i == cursor;
        if selected {
            sel_line = Some(lines.len());
        }
        let marker = if selected { ">" } else { " " };
        let name_style = if selected {
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .bg(theme::BG_SELECTION())
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
        // Ingredient row.
        lines.push(Line::from(Span::styled(
            format!("    {} · {}", e.skill.to_lowercase(), e.inputs),
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    lines.push(Line::raw(""));
    lines.push(hint("w/s", "select  Enter craft"));
    lines.push(hint("u", "close"));
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
    for (i, e) in stable.entries.iter().enumerate() {
        let selected = i == cursor;
        if selected {
            sel_line = Some(lines.len());
        }
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
            Style::default().fg(theme::TEXT_BRIGHT())
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{marker} {} {}", e.glyph, e.name), name_style),
            Span::styled(format!("  {}g", e.price), Style::default().fg(price_color)),
        ]));
        lines.push(Line::from(Span::styled(
            format!("    {}hp · {}atk", e.hp, e.attack),
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    lines.push(Line::raw(""));
    lines.push(hint("w/s", "select  Enter buy"));
    lines.push(hint("x", &format!("feed/tend ({}g)", stable.feed_cost)));
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
                .bg(theme::BG_SELECTION())
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
        Line::raw(""),
    ];
    // Villages come first in the destination list, then the islands.
    let village_count = super::archipelago::VILLAGES.len();
    for (i, (label, _room, here)) in portal.entries.iter().enumerate() {
        let selected = i == cursor;
        if selected {
            sel_line = Some(lines.len());
        }
        if i == village_count {
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
                .bg(theme::BG_SELECTION())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_BRIGHT())
        };
        lines.push(Line::from(Span::styled(
            format!("{marker} {label}{suffix}"),
            style,
        )));
    }
    lines.push(Line::raw(""));
    lines.push(hint("w/s", "select  Enter travel"));
    lines.push(hint("y", "close"));
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
                .bg(theme::BG_SELECTION())
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
    for (i, (label, value)) in view.appearance.iter().enumerate() {
        let selected = i == cursor;
        let marker = if selected { ">" } else { " " };
        let row_style = if selected {
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .bg(theme::BG_SELECTION())
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

fn footer_hints(view: &PlayerView) -> Vec<Line<'static>> {
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
    if view.corpse_here && view.can_resurrect {
        lines.push(hint("g", "resurrect the fallen"));
    }
    if view.in_combat_with.is_some() {
        lines.push(hint("space/x", "strike"));
        lines.push(hint("1-9 0", "use ability"));
        lines.push(hint("z", "flee"));
    } else {
        lines.push(hint("wasd/arrows", "move"));
        let at_town_square = view.room_name == "Embergate - Town Square";
        if at_town_square && view.exits.iter().any(|(dir, _)| *dir == Dir::South) {
            lines.push(hint("s", "King's Road"));
        }
        // Vertical exits aren't on wasd, so spell out the stair keys only when
        // this room actually has a way up or down.
        let has_up = view.exits.iter().any(|(dir, _)| *dir == Dir::Up);
        let has_down = view.exits.iter().any(|(dir, _)| *dir == Dir::Down);
        let has_danger_down = view
            .exits
            .iter()
            .any(|(dir, label)| *dir == Dir::Down && label.contains("dangerous Frontier"));
        match (has_up, has_down) {
            (true, true) => lines.push(hint("< >", "climb up / go down")),
            (true, false) => lines.push(hint("<", "climb up")),
            (false, true) if has_danger_down => lines.push(hint(">", "dangerous Frontier")),
            (false, true) => lines.push(hint(">", "go down")),
            (false, false) => {}
        }
        lines.push(hint("space", "attack"));
        lines.push(hint("o", "look at things"));
    }
    lines.push(hint("c v t", "sheet abilities bag"));
    lines.push(hint("j k", "quests titles"));
    lines.push(hint("r f", "recall follow"));
    lines.push(hint("'", "say (local chat)"));
    if view.shop.is_some() {
        lines.push(hint("b", "shop"));
    }
    if view.stable.is_some() {
        lines.push(hint("p", "stable (pets)"));
    }
    if view.housing.is_some() {
        lines.push(hint("n", "housing ledger"));
    }
    if view.crafting.is_some() {
        lines.push(hint("u", "craft (station here)"));
    }
    if view.portal.is_some() {
        lines.push(hint("i", "the ways (portal)"));
    }
    lines.push(hint("m", "world atlas"));
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

    let mut events = collapsed_recent_log_lines(view, width);
    if events.is_empty() {
        events.push(Line::from(Span::styled(
            "  no recent events",
            Style::default().fg(theme::TEXT_FAINT()),
        )));
    }

    let event_h = height.saturating_sub(1);
    let mut lines = vec![section("Recent")];
    lines.extend(events.into_iter().take(event_h));
    lines.truncate(height);
    lines
}

fn collapsed_recent_log_lines(view: &PlayerView, width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
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
        lines.extend(wrapped_log_line(line.kind, &text, width));
    }
    lines
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
        let selected = i == cursor;
        if selected {
            sel_line = Some(lines.len());
        }
        let following = view.following == Some(occ.user_id);
        let marker = if selected { "> " } else { "  " };
        let tag = if !occ.alive {
            "fallen"
        } else if following {
            "follow"
        } else if occ.in_combat {
            "fight"
        } else {
            ""
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
            &name,
            occ.hp,
            occ.max_hp,
            Style::default().fg(color).add_modifier(weight),
            12,
            tag,
        ));
    }
    // Profile the highlighted adventurer: show their chosen bio.
    if let Some(occ) = view.occupants.get(cursor)
        && !occ.bio.is_empty()
    {
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
    lines.push(Line::raw(""));
    lines.push(hint("w/s", "select  Enter follow/stop"));
    if view.following.is_some() {
        lines.push(hint("x", "stop following"));
    }
    lines.push(hint("f", "close"));
    (lines, sel_line)
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
/// from plain room text the way loot does. A fountain you drink from reads
/// green; vendors and usables you trade/act at read gold; purely lookable
/// scenery (a plaque, a vista) reads a softer cyan "examine me".
fn interactable_color(kind: &str) -> ratatui::style::Color {
    match kind {
        "fountain" => theme::SUCCESS(),
        "bank" | "board" | "stable" | "clerk" => theme::AMBER_GLOW(),
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
    matches!(kind, "fountain" | "bank" | "board" | "stable" | "clerk") || is_craft_station(kind)
}

#[cfg(test)]
mod tests {
    use super::{compare_span, fit, inventory_item_tag, meter, score_dots, scroll_offset};
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn scroll_offset_keeps_the_selection_visible_in_a_long_list() {
        // A 40-row list in a 10-tall window: the highlighted row must always land
        // inside the visible window [off, off+height) so nothing you're on scrolls
        // off-screen (the bug: titles/inventory ran off the bottom).
        let (total, height) = (40usize, 10usize);
        let mut off = 0;
        for sel in 0..total {
            off = scroll_offset(off, total, Some(sel), height);
            assert!(
                sel >= off && sel < off + height,
                "row {sel} fell outside window [{off}, {})",
                off + height
            );
            assert!(off <= total - height, "offset never overscrolls");
        }
    }

    #[test]
    fn compare_span_colours_upgrades_and_downgrades() {
        assert!(compare_span(None).is_none());
        assert!(compare_span(Some(18)).is_some(), "an upgrade shows a tag");
        assert!(compare_span(Some(-12)).is_some(), "a downgrade shows a tag");
    }

    #[test]
    fn score_dots_scale_from_empty_to_full() {
        assert_eq!(score_dots(3).chars().filter(|c| *c == '●').count(), 1);
        assert_eq!(score_dots(10).chars().filter(|c| *c == '●').count(), 4);
        assert_eq!(score_dots(18), "●●●●●●●●");
        // Every rating is exactly eight cells wide.
        for v in 0..=18 {
            assert_eq!(score_dots(v).chars().count(), 8);
        }
    }

    #[test]
    fn meter_fills_proportionally_and_clamps() {
        assert_eq!(meter(0, 100, 10), "░░░░░░░░░░");
        assert_eq!(meter(100, 100, 10), "██████████");
        assert_eq!(meter(50, 100, 10), "█████░░░░░");
        // Degenerate inputs never panic or overflow the width.
        assert_eq!(meter(5, 0, 6), "░░░░░░");
        assert_eq!(meter(999, 100, 6), "██████");
    }

    #[test]
    fn fit_pads_short_names_and_ellipsizes_long_ones() {
        assert_eq!(UnicodeWidthStr::width(fit("Goblin", 10).as_str()), 10);
        assert_eq!(fit("Goblin", 10), "Goblin    ");
        let long = fit("Ancient Frost Wyrm", 8);
        assert_eq!(UnicodeWidthStr::width(long.as_str()), 8);
        assert!(long.ends_with('…'));
    }

    #[test]
    fn equipped_inventory_tags_show_the_slot() {
        assert_eq!(inventory_item_tag(true, Some("weapon")), " [worn weapon]");
        assert_eq!(inventory_item_tag(true, Some("chest")), " [worn chest]");
        assert_eq!(inventory_item_tag(false, Some("ring")), " (ring)");
    }
}
