//! Rendering for the Green Dragon door: the live game page and the Games-hub
//! landing card. Pure presentation — everything is read off [`State`] getters
//! and the [`Character`]; no game logic lives here.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::app::common::theme;
use crate::app::door::landing;

use super::data;
use super::model::Character;
use super::state::{Mode, State};

/// Draw the live Green Dragon game (called when a character is loaded).
pub fn draw_page(frame: &mut Frame, area: Rect, state: &State) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::SUCCESS()))
        .title(Span::styled(
            " Legend of the Green Dragon ",
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width < 30 || inner.height < 10 {
        frame.render_widget(
            Paragraph::new("Terminal too small for Legend of the Green Dragon"),
            inner,
        );
        return;
    }

    let Some(c) = state.character() else {
        frame.render_widget(
            Paragraph::new("Loading your character from the realm...")
                .alignment(Alignment::Center),
            inner,
        );
        return;
    };

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(30), Constraint::Min(0)])
        .split(inner);

    draw_stats(frame, cols[0], c);
    draw_main(frame, cols[1], state, c);
}

fn draw_stats(frame: &mut Frame, area: Rect, c: &Character) {
    let bright = Style::default().fg(theme::TEXT_BRIGHT());
    let dim = Style::default().fg(theme::TEXT_DIM());
    let gold = Style::default().fg(theme::BADGE_GOLD());

    let stat = |label: &str, value: String, value_style: Style| {
        Line::from(vec![
            Span::styled(format!("{label:<9}"), dim),
            Span::styled(value, value_style),
        ])
    };

    let exp_target = c.exp_for_next_level();
    let lines = vec![
        Line::from(Span::styled(
            c.name.clone(),
            bright.add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
        stat("Level", c.level.to_string(), bright),
        stat(
            "HP",
            format!("{}/{}", c.hitpoints, c.max_hitpoints()),
            Style::default().fg(theme::SUCCESS()),
        ),
        stat("Attack", c.attack().to_string(), bright),
        stat("Defense", c.defense().to_string(), bright),
        Line::raw(""),
        stat("Weapon", data::weapon_name(c.weapon_tier).to_string(), bright),
        stat("Armor", data::armor_name(c.armor_tier).to_string(), bright),
        Line::raw(""),
        stat("Gold", c.gold.to_string(), gold),
        stat("Bank", c.gold_in_bank.to_string(), gold),
        stat(
            "Exp",
            if c.level >= data::MAX_LEVEL {
                format!("{}", c.experience)
            } else {
                format!("{}/{}", c.experience, exp_target)
            },
            dim,
        ),
        stat("Turns", c.turns.to_string(), bright),
        stat("Dragons", c.dragon_kills.to_string(), gold),
        stat("DK pts", c.dragon_points.to_string(), gold),
    ];

    let block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(Style::default().fg(theme::TEXT_FAINT()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_main(frame: &mut Frame, area: Rect, state: &State, c: &Character) {
    // Reserve the bottom for the message log; the rest is the active panel.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(4), Constraint::Length(9)])
        .split(area);

    draw_panel(frame, rows[0], state, c);
    draw_log(frame, rows[1], state);
}

fn draw_panel(frame: &mut Frame, area: Rect, state: &State, c: &Character) {
    let mut lines = vec![Line::from(Span::styled(
        panel_title(state.mode()),
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    ))];

    // Fight panels get a foe banner above the action list.
    if state.mode() == Mode::Fight
        && let Some(enc) = state.encounter()
    {
        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::styled(
                enc.name.clone(),
                Style::default()
                    .fg(theme::ERROR())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  wields {}", enc.weapon),
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Foe HP ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                format!("{}/{}", enc.hp, enc.max_hp),
                Style::default().fg(theme::ERROR()),
            ),
            Span::styled("   Your HP ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                format!("{}/{}", c.hitpoints, c.max_hitpoints()),
                Style::default().fg(theme::SUCCESS()),
            ),
        ]));
    }

    if state.mode() == Mode::Graveyard {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "You are dead. Rest here until a new day dawns and you rise renewed.",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }

    if state.mode() == Mode::Gypsy {
        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::styled("Dragon points  ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                c.dragon_points.to_string(),
                Style::default()
                    .fg(theme::BADGE_GOLD())
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(Span::styled(
            format!(
                "Boons so far: +{} HP, +{} attack, +{} defense, +{} daily fights",
                c.dragon_hp_bonus, c.dragon_attack_bonus, c.dragon_defense_bonus, c.dragon_turn_bonus
            ),
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }

    lines.push(Line::raw(""));
    for (i, (label, enabled)) in state.menu().into_iter().enumerate() {
        let selected = i == state.cursor();
        let style = match (selected, enabled) {
            (true, true) => Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD | Modifier::REVERSED),
            (true, false) => Style::default()
                .fg(theme::TEXT_FAINT())
                .add_modifier(Modifier::REVERSED),
            (false, true) => Style::default().fg(theme::TEXT_BRIGHT()),
            (false, false) => Style::default().fg(theme::TEXT_FAINT()),
        };
        let marker = if selected { "> " } else { "  " };
        lines.push(Line::from(Span::styled(format!("{marker}{label}"), style)));
    }

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        controls_hint(state.mode()),
        Style::default().fg(theme::TEXT_FAINT()),
    )));

    let block = Block::default().borders(Borders::NONE);
    let inner = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(block.inner(area))[1];
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn draw_log(frame: &mut Frame, area: Rect, state: &State) {
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(theme::TEXT_FAINT()))
        .title(Span::styled(
            " Recent events ",
            Style::default().fg(theme::TEXT_DIM()),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines: Vec<Line> = state
        .log_lines()
        .map(|l| Line::from(Span::styled(l.to_string(), Style::default().fg(theme::TEXT()))))
        .collect();
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn panel_title(mode: Mode) -> &'static str {
    match mode {
        Mode::Loading => "Entering the realm...",
        Mode::Village => "The village of Duskmere",
        Mode::Forest => "The Forest",
        Mode::Fight => "Battle!",
        Mode::WeaponShop => "Ironroost Weapons",
        Mode::ArmorShop => "Duskmail Armoury",
        Mode::Healer => "The Mendery",
        Mode::Bank => "The Coinvault",
        Mode::Training => "The Proving Yard",
        Mode::Gypsy => "The Gypsy's Tent",
        Mode::Graveyard => "The Graveyard",
    }
}

fn controls_hint(mode: Mode) -> &'static str {
    match mode {
        Mode::Fight => "up/down select   Enter act   Esc flee",
        Mode::Village => "up/down move   Enter choose   Esc leave the game",
        _ => "up/down move   Enter choose   Esc back to village",
    }
}

/// Two-column Green Dragon landing card for the Games hub.
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
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled(
            "An open-source remake of LORD ",
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "(Legend of the Green Dragon)",
            Style::default().fg(theme::AMBER_DIM()),
        ),
    ]));
    lines.push(Line::from(Span::styled(
        "Hunt the forest, train against the masters, gear up, and slay the Green Dragon. Your character persists.",
        Style::default().fg(theme::TEXT_DIM()),
    )));
    lines.push(Line::raw(""));
    lines.push(landing::heading("The Loop"));
    lines.push(landing::stat("Forest", "fight creatures for gold and experience", 10));
    lines.push(landing::stat("Masters", "beat your level master to advance", 10));
    lines.push(landing::stat("Dragon", "reach level 15, then end the run in glory", 10));
    lines.push(Line::raw(""));
    lines.push(landing::heading("Enter"));
    lines.push(landing::action(">", "Enter", "step into the village", theme::SUCCESS()));
    lines.push(landing::action(" ", "d", "reset your character", theme::ERROR()));
    lines.push(Line::raw(""));
    lines.push(landing::heading("Once Inside"));
    lines.push(landing::hint("up/down", "move the menu cursor", 10));
    lines.push(landing::hint("Enter", "choose", 10));
    lines.push(landing::hint("Esc", "back out / leave", 10));

    if delete_confirm {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "Delete your Green Dragon character?",
            Style::default()
                .fg(theme::ERROR())
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(vec![
            Span::styled("Enter/Y", Style::default().fg(theme::ERROR())),
            Span::styled(" confirm  ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled("N/Esc", Style::default().fg(theme::AMBER())),
            Span::styled(" cancel", Style::default().fg(theme::TEXT_DIM())),
        ]));
    } else {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "Esc leaves the game back to this gate.",
            Style::default().fg(theme::TEXT_FAINT()),
        )));
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn title_art() -> Vec<Line<'static>> {
    [
        "  ___                      ___                         ",
        " / __|_ _ ___ ___ _ _    |   \\ _ _ __ _ __ _ ___ _ _  ",
        "| (_ | '_/ -_) -_) ' \\   | |) | '_/ _` / _` / _ \\ ' \\ ",
        " \\___|_| \\___\\___|_||_|  |___/|_| \\__,_\\__, \\___/_||_|",
        "                                       |___/          ",
    ]
    .into_iter()
    .map(|line| {
        Line::from(Span::styled(
            line,
            Style::default()
                .fg(theme::SUCCESS())
                .add_modifier(Modifier::BOLD),
        ))
    })
    .collect()
}
