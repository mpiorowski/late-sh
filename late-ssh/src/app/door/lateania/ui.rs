// Rendering for Lateania. Reads the cached per-session snapshot and paints a
// two-column view: the scrolling adventure log on the left, a context side panel
// on the right (room / character / abilities / inventory / shop). Before a class
// is chosen it shows the class-selection screen. Lock-free; never awaits.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::common::theme;
use crate::usernames::UsernameLookup;

use super::{
    classes::Class,
    state::{Panel, State},
    svc::{LogKind, PlayerView},
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

fn draw_class_select(frame: &mut Frame, area: Rect, _view: &PlayerView) {
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
    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_log(frame: &mut Frame, area: Rect, view: &PlayerView) {
    let lines = wrapped_log_tail(view, area.width as usize, area.height as usize);
    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_side(
    frame: &mut Frame,
    area: Rect,
    state: &State,
    view: &PlayerView,
    usernames: &UsernameLookup<'_>,
) {
    let lines = match state.panel() {
        Panel::Room => room_panel(view, usernames),
        Panel::Character => character_panel(view),
        Panel::Abilities => abilities_panel(view),
        Panel::Inventory => inventory_panel(view, state.cursor()),
        Panel::Shop => shop_panel(view, state.cursor()),
    };
    frame.render_widget(Paragraph::new(lines), area);
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
        ]),
        Line::from(vec![
            Span::styled("HP  ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                format!("{}/{}", view.hp, view.max_hp),
                Style::default()
                    .fg(hp_color(view.hp, view.max_hp))
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("{:<4}", short_res(&view.resource_name)),
                Style::default().fg(theme::TEXT_DIM()),
            ),
            Span::styled(
                format!("{}/{}", view.resource, view.max_resource),
                Style::default().fg(theme::MENTION()),
            ),
        ]),
        Line::from(vec![
            Span::styled("gold ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                format!("{}", view.gold),
                Style::default().fg(theme::BADGE_GOLD()),
            ),
        ]),
    ]
}

fn room_panel(view: &PlayerView, usernames: &UsernameLookup<'_>) -> Vec<Line<'static>> {
    let mut lines = vitals(view);
    lines.push(Line::raw(""));
    lines.push(section("Here"));
    lines.push(Line::from(Span::styled(
        format!("  {}", view.zone),
        Style::default().fg(theme::TEXT()),
    )));
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
        lines.push(section("Foes"));
        for mob in &view.mobs {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {} ", mob.name),
                    Style::default().fg(theme::ERROR()),
                ),
                Span::styled(
                    format!("{}/{}", mob.hp, mob.max_hp),
                    Style::default().fg(theme::TEXT_DIM()),
                ),
            ]));
        }
    }
    if !view.occupants.is_empty() {
        lines.push(section("Adventurers here"));
        for occ in &view.occupants {
            let name = usernames
                .get(&occ.user_id)
                .cloned()
                .unwrap_or_else(|| "adventurer".to_string());
            let tag = if occ.in_combat { " (fighting)" } else { "" };
            lines.push(Line::from(Span::styled(
                format!("  {name}{tag}"),
                Style::default().fg(theme::SUCCESS()),
            )));
        }
    }
    lines.push(Line::raw(""));
    lines.extend(footer_hints(view));
    lines
}

fn character_panel(view: &PlayerView) -> Vec<Line<'static>> {
    let mut lines = vitals(view);
    lines.push(Line::raw(""));
    lines.push(section("Combat"));
    lines.push(stat("attack", view.attack.to_string()));
    lines.push(stat("armor", view.armor.to_string()));
    lines.push(Line::raw(""));
    lines.push(section("Trait"));
    lines.push(Line::from(Span::styled(
        format!("  {}", view.trait_name),
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    )));
    lines.extend(wrap(&view.trait_desc, 30));
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
        lines.push(Line::from(Span::styled(
            format!("{marker} {}{}", it.name, tag),
            style,
        )));
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
        lines.push(Line::from(vec![
            Span::styled(format!("{marker} {}", e.name), name_style),
            Span::styled(format!("  {}g", e.price), Style::default().fg(price_color)),
        ]));
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
        lines.push(hint("space", "attack  o look"));
    }
    lines.push(hint("c v t", "sheet abilities bag"));
    if view.shop.is_some() {
        lines.push(hint("b", "shop"));
    }
    lines.push(hint("q", "leave"));
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

fn wrapped_log_line(kind: LogKind, text: &str, width: usize) -> Vec<Line<'static>> {
    let color = match kind {
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

fn rarity_color(rarity: &str) -> ratatui::style::Color {
    match rarity {
        "uncommon" => theme::SUCCESS(),
        "rare" => theme::MENTION(),
        "epic" => theme::AMBER_GLOW(),
        "legendary" => theme::BADGE_GOLD(),
        _ => theme::TEXT(),
    }
}
